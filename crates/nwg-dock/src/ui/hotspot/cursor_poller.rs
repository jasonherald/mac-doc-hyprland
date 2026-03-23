use crate::config::DockConfig;
use crate::dock_windows::MonitorDock;
use crate::state::DockState;
use gtk4::glib;
use gtk4::prelude::*;
use nwg_dock_common::compositor::{Compositor, WmMonitor};
use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::constants::EDGE_THRESHOLD;

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
    let cached_monitors: Rc<RefCell<Vec<WmMonitor>>> =
        Rc::new(RefCell::new(compositor.list_monitors().unwrap_or_default()));
    let monitor_refresh_counter = Rc::new(RefCell::new(0u32));
    let last_outputs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(
        docks
            .borrow()
            .iter()
            .map(|d| d.output_name.clone())
            .collect(),
    ));

    glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
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
            if *count >= 50 || topology_changed {
                *count = 0;
                if let Ok(m) = compositor.list_monitors() {
                    *cached_monitors.borrow_mut() = m;
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
