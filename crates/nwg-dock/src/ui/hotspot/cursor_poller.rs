use crate::config::DockConfig;
use crate::dock_windows::MonitorDock;
use crate::state::DockState;
use gtk4::glib;
use gtk4::prelude::*;
use nwg_dock_common::compositor::{Compositor, WmMonitor};
use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::constants::EDGE_THRESHOLD;

/// Cursor polling interval in milliseconds.
const CURSOR_POLL_INTERVAL_MS: u64 = 200;

/// Number of poll cycles between monitor cache refreshes (~10 seconds).
const MONITOR_REFRESH_POLLS: u32 = 50;

/// Starts a cursor position poller that shows/hides dock windows
/// based on whether the cursor is near the screen edge or inside the dock.
///
/// Uses compositor IPC cursor tracking (Hyprland `j/cursorpos`).
/// Monitor↔window mapping uses output connector names, not array indices.
pub(super) fn start_cursor_poller(
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    compositor: &Rc<dyn Compositor>,
) {
    let docks = Rc::clone(per_monitor);
    let position = config.position;
    let hide_timeout = config.hide_timeout;
    let state = Rc::clone(state);
    let compositor = Rc::clone(compositor);
    // Track when cursor last left the dock area (for hide delay)
    let left_at: Rc<RefCell<Option<std::time::Instant>>> = Rc::new(RefCell::new(None));

    // Cache monitors — refreshed periodically and immediately on topology changes
    let cached_monitors: Rc<RefCell<Vec<WmMonitor>>> = Rc::new(RefCell::new(
        match compositor.list_monitors() {
            Ok(m) => m,
            Err(e) => {
                log::warn!("Initial monitor list failed: {}", e);
                Vec::new()
            }
        },
    ));
    let monitor_refresh_counter = Rc::new(RefCell::new(0u32));
    let last_outputs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(
        docks
            .borrow()
            .iter()
            .map(|d| d.output_name.clone())
            .collect(),
    ));

    glib::timeout_add_local(std::time::Duration::from_millis(CURSOR_POLL_INTERVAL_MS), move || {
        let cursor = match compositor.get_cursor_position() {
            Some((x, y)) => CursorPos { x, y },
            None => return glib::ControlFlow::Continue,
        };

        // Detect topology change: output names changed means reconciliation happened
        let current_outputs: Vec<String> = docks
            .borrow()
            .iter()
            .map(|d| d.output_name.clone())
            .collect();
        let topology_changed = {
            let mut last = last_outputs.borrow_mut();
            if *last != current_outputs {
                *last = current_outputs;
                true
            } else {
                false
            }
        };

        // Refresh monitor cache every ~10 seconds or immediately on topology change
        {
            let mut count = monitor_refresh_counter.borrow_mut();
            *count += 1;
            if *count >= MONITOR_REFRESH_POLLS || topology_changed {
                *count = 0;
                match compositor.list_monitors() {
                    Ok(m) => *cached_monitors.borrow_mut() = m,
                    Err(e) => log::debug!("Monitor cache refresh failed: {}", e),
                }
            }
        }
        let monitors = cached_monitors.borrow();

        let any_visible = docks.borrow().iter().any(|d| d.win.is_visible());

        if !any_visible {
            handle_hidden_dock(&cursor, &monitors, position, &docks, &left_at);
        } else {
            handle_visible_dock(
                &cursor,
                &monitors,
                position,
                &docks,
                &state,
                &left_at,
                hide_timeout,
            );
        }

        glib::ControlFlow::Continue
    });
}

/// Handles cursor polling when the dock is hidden: shows the dock if cursor is at edge.
fn handle_hidden_dock(
    cursor: &CursorPos,
    monitors: &[WmMonitor],
    position: crate::config::Position,
    docks: &Rc<RefCell<Vec<MonitorDock>>>,
    left_at: &Rc<RefCell<Option<std::time::Instant>>>,
) {
    if is_cursor_at_edge(cursor, monitors, position)
        && let Some(mon_name) = find_cursor_monitor_name(cursor, monitors)
    {
        show_on_monitor_only_by_name(docks, &mon_name);
        *left_at.borrow_mut() = None;
    }
}

/// Handles cursor polling when the dock is visible: hides after timeout if cursor leaves.
fn handle_visible_dock(
    cursor: &CursorPos,
    monitors: &[WmMonitor],
    position: crate::config::Position,
    docks: &Rc<RefCell<Vec<MonitorDock>>>,
    state: &Rc<RefCell<DockState>>,
    left_at: &Rc<RefCell<Option<std::time::Instant>>>,
    hide_timeout: u64,
) {
    let in_dock_area = is_cursor_in_visible_dock(cursor, docks, monitors, position);
    let at_edge = is_cursor_at_edge(cursor, monitors, position);

    // Don't hide while a popover menu is open or a drag is in progress
    let s = state.borrow();
    let dragging = s.drag_source_index.is_some();
    let keep_visible = s.popover_open || dragging;
    drop(s);

    update_drag_state(state, dragging, in_dock_area, at_edge);

    // Cursor is at edge of a different monitor — migrate dock there (macOS behavior)
    if at_edge
        && !in_dock_area
        && !keep_visible
        && let Some(mon_name) = find_cursor_monitor_name(cursor, monitors)
    {
        show_on_monitor_only_by_name(docks, &mon_name);
        *left_at.borrow_mut() = None;
        return;
    }

    if in_dock_area || at_edge || keep_visible {
        *left_at.borrow_mut() = None;
    } else {
        check_hide_timer(docks, left_at, hide_timeout);
    }
}

use super::show_on_monitor_only_by_name;

/// Tracks whether cursor is outside dock during a drag operation.
fn update_drag_state(
    state: &Rc<RefCell<DockState>>,
    dragging: bool,
    in_dock_area: bool,
    at_edge: bool,
) {
    if dragging {
        let was_outside = state.borrow().drag_outside_dock;
        let now_outside = !in_dock_area && !at_edge;
        if was_outside != now_outside {
            state.borrow_mut().drag_outside_dock = now_outside;
        }
    }
}

/// Starts or checks the hide timer, hiding all dock windows when expired.
fn check_hide_timer(
    docks: &Rc<RefCell<Vec<MonitorDock>>>,
    left_at: &Rc<RefCell<Option<std::time::Instant>>>,
    hide_timeout: u64,
) {
    let mut left = left_at.borrow_mut();
    match *left {
        None => *left = Some(std::time::Instant::now()),
        Some(when) if when.elapsed().as_millis() >= hide_timeout as u128 => {
            log::debug!("Cursor left dock area, hiding");
            for dock in docks.borrow().iter() {
                dock.win.set_visible(false);
            }
            *left = None;
        }
        _ => {} // timer running but not expired
    }
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
            crate::config::Position::Top => cursor.y < mon.y + EDGE_THRESHOLD,
            crate::config::Position::Left => cursor.x < mon.x + EDGE_THRESHOLD,
            crate::config::Position::Right => cursor.x >= mon.x + mon.width - EDGE_THRESHOLD,
        };

        if at_edge {
            return true;
        }
    }
    false
}

/// Returns the output name of the monitor containing the cursor, or None.
fn find_cursor_monitor_name(cursor: &CursorPos, monitors: &[WmMonitor]) -> Option<String> {
    for mon in monitors {
        let in_x = cursor.x >= mon.x && cursor.x < mon.x + mon.width;
        let in_y = cursor.y >= mon.y && cursor.y < mon.y + mon.height;
        if in_x && in_y {
            return Some(mon.name.clone());
        }
    }
    None
}

/// Computes the (x, y) origin of a dock window on a given monitor based on position.
fn dock_bounds_for_position(
    mon: &WmMonitor,
    w: i32,
    h: i32,
    position: crate::config::Position,
) -> (i32, i32) {
    match position {
        crate::config::Position::Bottom => (mon.x + (mon.width - w) / 2, mon.y + mon.height - h),
        crate::config::Position::Top => (mon.x + (mon.width - w) / 2, mon.y),
        crate::config::Position::Left => (mon.x, mon.y + (mon.height - h) / 2),
        crate::config::Position::Right => (mon.x + mon.width - w, mon.y + (mon.height - h) / 2),
    }
}

/// Checks if the cursor is within the bounds of the visible dock window.
/// Matches dock windows to monitors by output name (hotplug-safe).
fn is_cursor_in_visible_dock(
    cursor: &CursorPos,
    docks: &Rc<RefCell<Vec<MonitorDock>>>,
    monitors: &[WmMonitor],
    position: crate::config::Position,
) -> bool {
    let dock_list = docks.borrow();
    for dock in dock_list.iter() {
        if !dock.win.is_visible() || dock.win.surface().is_none() {
            continue;
        }
        let w = dock.win.width();
        let h = dock.win.height();
        if w == 0 || h == 0 {
            continue;
        }
        // Find the WmMonitor matching this dock's output name
        let Some(mon) = monitors.iter().find(|m| m.name == dock.output_name) else {
            log::debug!(
                "No monitor data for dock output '{}', skipping bounds check",
                dock.output_name
            );
            continue;
        };
        let (dock_x, dock_y) = dock_bounds_for_position(mon, w, h, position);
        if cursor.x >= dock_x
            && cursor.x < dock_x + w
            && cursor.y >= dock_y
            && cursor.y < dock_y + h
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use nwg_dock_common::compositor::WmWorkspace;

    fn test_monitor(name: &str, x: i32, y: i32, w: i32, h: i32) -> WmMonitor {
        WmMonitor {
            id: 0,
            name: name.to_string(),
            x,
            y,
            width: w,
            height: h,
            scale: 1.0,
            focused: false,
            active_workspace: WmWorkspace::default(),
        }
    }

    #[test]
    fn edge_detection_bottom() {
        let monitors = vec![test_monitor("DP-1", 0, 0, 1920, 1080)];
        let at_edge = CursorPos { x: 960, y: 1079 };
        let not_edge = CursorPos { x: 960, y: 500 };
        assert!(is_cursor_at_edge(&at_edge, &monitors, crate::config::Position::Bottom));
        assert!(!is_cursor_at_edge(&not_edge, &monitors, crate::config::Position::Bottom));
    }

    #[test]
    fn edge_detection_top() {
        let monitors = vec![test_monitor("DP-1", 0, 0, 1920, 1080)];
        let at_edge = CursorPos { x: 960, y: 1 };
        let not_edge = CursorPos { x: 960, y: 500 };
        assert!(is_cursor_at_edge(&at_edge, &monitors, crate::config::Position::Top));
        assert!(!is_cursor_at_edge(&not_edge, &monitors, crate::config::Position::Top));
    }

    #[test]
    fn find_monitor_by_cursor_position() {
        let monitors = vec![
            test_monitor("DP-1", 0, 0, 1920, 1080),
            test_monitor("HDMI-A-1", 1920, 0, 2560, 1440),
        ];
        assert_eq!(
            find_cursor_monitor_name(&CursorPos { x: 500, y: 500 }, &monitors).as_deref(),
            Some("DP-1")
        );
        assert_eq!(
            find_cursor_monitor_name(&CursorPos { x: 2000, y: 500 }, &monitors).as_deref(),
            Some("HDMI-A-1")
        );
        assert!(find_cursor_monitor_name(&CursorPos { x: 5000, y: 5000 }, &monitors).is_none());
    }

    #[test]
    fn dock_bounds_bottom_center() {
        let mon = test_monitor("DP-1", 0, 0, 1920, 1080);
        let (x, y) = dock_bounds_for_position(&mon, 800, 50, crate::config::Position::Bottom);
        assert_eq!(x, (1920 - 800) / 2);
        assert_eq!(y, 1080 - 50);
    }

    #[test]
    fn dock_bounds_with_offset_monitor() {
        let mon = test_monitor("HDMI-A-1", 1920, 0, 2560, 1440);
        let (x, y) = dock_bounds_for_position(&mon, 800, 50, crate::config::Position::Bottom);
        assert_eq!(x, 1920 + (2560 - 800) / 2);
        assert_eq!(y, 1440 - 50);
    }
}
