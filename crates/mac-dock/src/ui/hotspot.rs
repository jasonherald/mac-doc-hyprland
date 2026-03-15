use crate::config::DockConfig;
use crate::state::DockState;
use dock_common::hyprland::ipc;
use dock_common::hyprland::types::HyprMonitor;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Edge detection threshold in pixels from the screen bottom.
const EDGE_THRESHOLD: i32 = 2;

/// Starts a cursor position poller that shows/hides dock windows
/// based on whether the cursor is near the screen edge or inside the dock.
///
/// Uses Hyprland IPC cursor tracking instead of GTK hotspot windows.
pub fn start_cursor_poller(
    dock_windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    config: &DockConfig,
    _state: &Rc<RefCell<DockState>>,
) {
    let windows = Rc::clone(dock_windows);
    let position = config.position.clone();
    let hide_timeout = config.hide_timeout;
    // Track when cursor last left the dock area (for hide delay)
    let left_at: Rc<RefCell<Option<std::time::Instant>>> = Rc::new(RefCell::new(None));

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        let cursor = match get_cursor_pos() {
            Some(c) => c,
            None => return glib::ControlFlow::Continue,
        };

        let monitors = match ipc::list_monitors() {
            Ok(m) => m,
            Err(_) => return glib::ControlFlow::Continue,
        };

        let any_visible = windows.borrow().iter().any(|w| w.is_visible());

        if !any_visible {
            // Dock is hidden — check if cursor is at the screen edge to show
            if is_cursor_at_edge(&cursor, &monitors, &position) {
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
            let in_dock_area = is_cursor_in_visible_dock(&cursor, &windows);
            let at_edge = is_cursor_at_edge(&cursor, &monitors, &position);

            if in_dock_area || at_edge {
                // Cursor is in dock or at edge — reset hide timer
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

fn get_cursor_pos() -> Option<CursorPos> {
    let reply = ipc::hyprctl("j/cursorpos").ok()?;
    let val: serde_json::Value = serde_json::from_slice(&reply).ok()?;
    Some(CursorPos {
        x: val.get("x")?.as_i64()? as i32,
        y: val.get("y")?.as_i64()? as i32,
    })
}

fn is_cursor_at_edge(cursor: &CursorPos, monitors: &[HyprMonitor], position: &str) -> bool {
    for mon in monitors {
        let in_x = cursor.x >= mon.x && cursor.x < mon.x + mon.width;
        let in_y = cursor.y >= mon.y && cursor.y < mon.y + mon.height;
        if !in_x || !in_y {
            continue;
        }

        let at_edge = match position {
            "bottom" => cursor.y >= mon.y + mon.height - EDGE_THRESHOLD,
            "top" => cursor.y <= mon.y + EDGE_THRESHOLD,
            "left" => cursor.x <= mon.x + EDGE_THRESHOLD,
            "right" => cursor.x >= mon.x + mon.width - EDGE_THRESHOLD,
            _ => false,
        };

        if at_edge {
            return true;
        }
    }
    false
}

fn find_cursor_monitor(cursor: &CursorPos, monitors: &[HyprMonitor]) -> Option<usize> {
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
/// Uses the window's allocated size and its monitor's position to compute bounds.
fn is_cursor_in_visible_dock(
    cursor: &CursorPos,
    windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
) -> bool {
    let wins = windows.borrow();
    for win in wins.iter() {
        if !win.is_visible() {
            continue;
        }
        // Get the layer surface geometry from Hyprland
        // Fall back to a generous check based on monitor bottom area
        let w = win.width();
        let h = win.height();
        if w == 0 || h == 0 {
            continue;
        }

        // The dock is centered at the bottom of its monitor.
        // We need the monitor's geometry to compute absolute position.
        // Use the surface allocation as an approximation.
        if win.surface().is_some()
            && let Ok(monitors) = ipc::list_monitors() {
                for mon in &monitors {
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
