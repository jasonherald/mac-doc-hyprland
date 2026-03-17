use crate::config::DockConfig;
use crate::state::DockState;
use dock_common::compositor::{Compositor, WmMonitor};
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Edge detection threshold in pixels from the screen bottom.
const EDGE_THRESHOLD: i32 = 2;

/// Starts a cursor position poller that shows/hides dock windows
/// based on whether the cursor is near the screen edge or inside the dock.
///
/// Uses compositor IPC cursor tracking instead of GTK hotspot windows.
pub fn start_cursor_poller(
    dock_windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    compositor: &Rc<dyn Compositor>,
) {
    let windows = Rc::clone(dock_windows);
    let position = config.position;
    let hide_timeout = config.hide_timeout;
    let state = Rc::clone(state);
    let compositor = Rc::clone(compositor);
    // Track when cursor last left the dock area (for hide delay)
    let left_at: Rc<RefCell<Option<std::time::Instant>>> = Rc::new(RefCell::new(None));

    // Cache monitors — they rarely change during a session
    let cached_monitors: Rc<RefCell<Vec<WmMonitor>>> =
        Rc::new(RefCell::new(compositor.list_monitors().unwrap_or_default()));
    let monitor_refresh_counter = Rc::new(RefCell::new(0u32));

    glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
        let cursor = match compositor.get_cursor_position() {
            Some((x, y)) => CursorPos { x, y },
            None => return glib::ControlFlow::Continue,
        };

        // Refresh monitor cache every ~10 seconds (50 polls at 200ms)
        {
            let mut count = monitor_refresh_counter.borrow_mut();
            *count += 1;
            if *count >= 50 {
                *count = 0;
                if let Ok(m) = compositor.list_monitors() {
                    *cached_monitors.borrow_mut() = m;
                }
            }
        }
        let monitors = cached_monitors.borrow();

        let any_visible = windows.borrow().iter().any(|w| w.is_visible());

        if !any_visible {
            // Dock is hidden — check if cursor is at the screen edge to show
            if is_cursor_at_edge(&cursor, &monitors, position) {
                if let Some(mon_idx) = find_cursor_monitor(&cursor, &monitors) {
                    let wins = windows.borrow();
                    if mon_idx < wins.len() {
                        log::debug!("Cursor at edge, showing dock on monitor {}", mon_idx);
                        wins[mon_idx].set_visible(true);
                    }
                }
                *left_at.borrow_mut() = None;
            }
        } else {
            // Dock is visible — check if cursor is inside dock area or at edge
            let in_dock_area = is_cursor_in_visible_dock(&cursor, &windows, &monitors);
            let at_edge = is_cursor_at_edge(&cursor, &monitors, position);

            // Don't hide while a popover menu is open or a drag is in progress
            let s = state.borrow();
            let dragging = s.drag_source_index.is_some();
            let keep_visible = s.popover_open || dragging;
            drop(s);

            // Track whether cursor is outside dock during a drag
            if dragging {
                let was_outside = state.borrow().drag_outside_dock;
                let now_outside = !in_dock_area && !at_edge;
                if was_outside != now_outside {
                    state.borrow_mut().drag_outside_dock = now_outside;
                }
            }

            if in_dock_area || at_edge || keep_visible {
                // Cursor is in dock, at edge, or menu open — reset hide timer
                *left_at.borrow_mut() = None;
            } else {
                // Cursor left the dock area — start or check hide timer
                let mut left = left_at.borrow_mut();
                if left.is_none() {
                    *left = Some(std::time::Instant::now());
                } else if left.unwrap().elapsed().as_millis() >= hide_timeout as u128 {
                    // Timer expired — hide all dock windows
                    log::debug!("Cursor left dock area, hiding");
                    for win in windows.borrow().iter() {
                        win.set_visible(false);
                    }
                    *left = None;
                }
            }
        }

        glib::ControlFlow::Continue
    });
}

#[derive(Debug)]
struct CursorPos {
    x: i32,
    y: i32,
}

fn is_cursor_at_edge(
    cursor: &CursorPos,
    monitors: &[WmMonitor],
    position: crate::config::Position,
) -> bool {
    for mon in monitors {
        let in_x = cursor.x >= mon.x && cursor.x < mon.x + mon.width;
        let in_y = cursor.y >= mon.y && cursor.y < mon.y + mon.height;
        if !in_x || !in_y {
            continue;
        }

        let at_edge = match position {
            crate::config::Position::Bottom => cursor.y >= mon.y + mon.height - EDGE_THRESHOLD,
            crate::config::Position::Top => cursor.y <= mon.y + EDGE_THRESHOLD,
            crate::config::Position::Left => cursor.x <= mon.x + EDGE_THRESHOLD,
            crate::config::Position::Right => cursor.x >= mon.x + mon.width - EDGE_THRESHOLD,
        };

        if at_edge {
            return true;
        }
    }
    false
}

fn find_cursor_monitor(cursor: &CursorPos, monitors: &[WmMonitor]) -> Option<usize> {
    for (i, mon) in monitors.iter().enumerate() {
        let in_x = cursor.x >= mon.x && cursor.x < mon.x + mon.width;
        let in_y = cursor.y >= mon.y && cursor.y < mon.y + mon.height;
        if in_x && in_y {
            return Some(i);
        }
    }
    None
}

/// Checks if the cursor is within the bounds of any visible dock window.
/// Uses the window's allocated size and monitor positions to compute bounds.
fn is_cursor_in_visible_dock(
    cursor: &CursorPos,
    windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    monitors: &[WmMonitor],
) -> bool {
    let wins = windows.borrow();
    for win in wins.iter() {
        if !win.is_visible() {
            continue;
        }
        let w = win.width();
        let h = win.height();
        if w == 0 || h == 0 {
            continue;
        }

        if win.surface().is_some() {
            for mon in monitors {
                // Check if this window is on this monitor
                // (dock centered at bottom of monitor)
                let dock_x = mon.x + (mon.width - w) / 2;
                let dock_y = mon.y + mon.height - h;

                let in_x = cursor.x >= dock_x && cursor.x < dock_x + w;
                let in_y = cursor.y >= dock_y && cursor.y <= mon.y + mon.height;

                if in_x && in_y {
                    return true;
                }
            }
        }
    }
    false
}
