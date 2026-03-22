use crate::config::DockConfig;
use crate::dock_windows::MonitorDock;
use crate::state::DockState;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use nwg_dock_common::compositor::{Compositor, WmMonitor};
use std::cell::RefCell;
use std::rc::Rc;

/// Edge detection threshold in pixels from the screen edge.
const EDGE_THRESHOLD: i32 = 2;

/// Thickness of the hotspot trigger window in pixels.
const HOTSPOT_THICKNESS: i32 = 4;

/// Shared state for creating hotspot windows on Sway during monitor hotplug.
/// Returned by `setup_autohide` when the compositor uses the hotspot approach.
pub struct HotspotContext {
    app: gtk4::Application,
    position: crate::config::Position,
    per_monitor: Rc<RefCell<Vec<MonitorDock>>>,
    left_at: Rc<RefCell<Option<std::time::Instant>>>,
}

impl HotspotContext {
    /// Creates a hotspot window for a newly added dock (called during reconciliation).
    pub fn add_hotspot_for_dock(&self, dock: &MonitorDock) {
        create_hotspot_window(&self.app, self.position, dock, &self.per_monitor, &self.left_at);
    }
}

/// Sets up autohide using the appropriate method for the compositor.
///
/// - Compositors with cursor position IPC (Hyprland): poll cursor position
/// - Compositors without (Sway): use thin GTK layer-shell hotspot windows
///
/// Returns a `HotspotContext` for the Sway path, which reconciliation uses
/// to create hotspot windows for hotplugged monitors.
pub fn setup_autohide(
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    compositor: &Rc<dyn Compositor>,
    app: &gtk4::Application,
) -> Option<Rc<HotspotContext>> {
    if compositor.supports_cursor_position() {
        start_cursor_poller(per_monitor, config, state, compositor);
        None
    } else {
        Some(start_hotspot_windows(per_monitor, config, state, app))
    }
}

// =============================================================================
// GTK hotspot approach (for Sway and other compositors without cursor IPC)
// =============================================================================

/// Creates thin layer-shell windows at the dock edge to trigger show on hover.
/// Uses GTK4 EventControllerMotion for enter/leave detection.
fn start_hotspot_windows(
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    app: &gtk4::Application,
) -> Rc<HotspotContext> {
    let hide_timeout = config.hide_timeout;
    let position = config.position;

    // Shared hide timer state
    let left_at: Rc<RefCell<Option<std::time::Instant>>> = Rc::new(RefCell::new(None));

    let ctx = Rc::new(HotspotContext {
        app: app.clone(),
        position,
        per_monitor: Rc::clone(per_monitor),
        left_at: Rc::clone(&left_at),
    });

    // Create hotspot windows for each current dock window
    for dock in per_monitor.borrow().iter() {
        create_hotspot_window(app, position, dock, per_monitor, &left_at);
    }

    // Poll the hide timer to actually hide dock windows
    let docks = Rc::clone(per_monitor);
    let state = Rc::clone(state);
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        let mut left = left_at.borrow_mut();
        if let Some(when) = *left {
            // Don't hide while a popover menu is open or drag in progress
            let s = state.borrow();
            let keep_visible = s.popover_open || s.drag_source_index.is_some();
            drop(s);

            if keep_visible {
                *left = None;
            } else if when.elapsed().as_millis() >= hide_timeout as u128 {
                log::debug!("Cursor left dock area, hiding (hotspot mode)");
                for dock in docks.borrow().iter() {
                    dock.win.set_visible(false);
                }
                *left = None;
            }
        }
        glib::ControlFlow::Continue
    });

    ctx
}

/// Creates a single hotspot trigger window for one monitor and attaches enter/leave handlers.
fn create_hotspot_window(
    app: &gtk4::Application,
    position: crate::config::Position,
    dock: &MonitorDock,
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    left_at: &Rc<RefCell<Option<std::time::Instant>>>,
) {
    let output_name = dock.output_name.clone();
    let docks = Rc::clone(per_monitor);

    // --- Create the hotspot trigger window ---
    let hotspot = gtk4::ApplicationWindow::new(app);
    hotspot.init_layer_shell();
    hotspot.set_namespace(Some("nwg-dock-hotspot"));
    setup_hotspot_layer(&hotspot, position);

    // Set hotspot on the same monitor as the dock window
    if let Some(mon) = dock.win.monitor() {
        hotspot.set_monitor(Some(&mon));
    }

    // Minimal content with near-zero opacity so compositor delivers input
    let hotspot_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    hotspot_box.add_css_class("dock-hotspot");
    hotspot.set_child(Some(&hotspot_box));

    // Load hotspot CSS once
    static CSS_LOADED: std::sync::Once = std::sync::Once::new();
    CSS_LOADED.call_once(|| {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(".dock-hotspot { background: rgba(0,0,0,0.01); }");
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        } else {
            log::error!("No display available for hotspot CSS provider");
        }
    });

    hotspot.present();

    // Hotspot enter → show dock on this monitor (by name)
    let docks_enter = Rc::clone(&docks);
    let name_enter = output_name.clone();
    let left_at_enter = Rc::clone(left_at);
    let motion = gtk4::EventControllerMotion::new();
    motion.connect_enter(move |_, _, _| {
        show_on_monitor_only_by_name(&docks_enter, &name_enter);
        *left_at_enter.borrow_mut() = None;
    });
    hotspot.add_controller(motion);

    // --- Attach enter/leave to the dock window ---
    // Dock enter → cancel hide timer
    let left_at_dock_enter = Rc::clone(left_at);
    let dock_motion = gtk4::EventControllerMotion::new();
    dock_motion.connect_enter(move |_, _, _| {
        *left_at_dock_enter.borrow_mut() = None;
    });
    dock.win.add_controller(dock_motion);

    // Dock leave → start hide timer
    let left_at_dock_leave = Rc::clone(left_at);
    let leave_motion = gtk4::EventControllerMotion::new();
    leave_motion.connect_leave(move |_| {
        *left_at_dock_leave.borrow_mut() = Some(std::time::Instant::now());
    });
    dock.win.add_controller(leave_motion);
}

/// Configures a hotspot window as a thin strip at the dock edge.
fn setup_hotspot_layer(win: &gtk4::ApplicationWindow, position: crate::config::Position) {
    use crate::config::Position;

    win.set_layer(gtk4_layer_shell::Layer::Overlay);
    win.set_exclusive_zone(-1);
    win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);

    match position {
        Position::Bottom => {
            win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
            win.set_anchor(gtk4_layer_shell::Edge::Left, true);
            win.set_anchor(gtk4_layer_shell::Edge::Right, true);
            win.set_size_request(-1, HOTSPOT_THICKNESS);
        }
        Position::Top => {
            win.set_anchor(gtk4_layer_shell::Edge::Top, true);
            win.set_anchor(gtk4_layer_shell::Edge::Left, true);
            win.set_anchor(gtk4_layer_shell::Edge::Right, true);
            win.set_size_request(-1, HOTSPOT_THICKNESS);
        }
        Position::Left => {
            win.set_anchor(gtk4_layer_shell::Edge::Left, true);
            win.set_anchor(gtk4_layer_shell::Edge::Top, true);
            win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
            win.set_size_request(HOTSPOT_THICKNESS, -1);
        }
        Position::Right => {
            win.set_anchor(gtk4_layer_shell::Edge::Right, true);
            win.set_anchor(gtk4_layer_shell::Edge::Top, true);
            win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
            win.set_size_request(HOTSPOT_THICKNESS, -1);
        }
    }
}

// =============================================================================
// IPC cursor poller approach (for Hyprland)
// =============================================================================

/// Starts a cursor position poller that shows/hides dock windows
/// based on whether the cursor is near the screen edge or inside the dock.
///
/// Uses compositor IPC cursor tracking (Hyprland `j/cursorpos`).
/// Monitor↔window mapping uses output connector names, not array indices.
fn start_cursor_poller(
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
    let last_dock_count = Rc::new(RefCell::new(docks.borrow().len()));

    glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
        let cursor = match compositor.get_cursor_position() {
            Some((x, y)) => CursorPos { x, y },
            None => return glib::ControlFlow::Continue,
        };

        // Detect topology change: dock count changed means reconciliation happened
        let current_dock_count = docks.borrow().len();
        let topology_changed = {
            let mut last = last_dock_count.borrow_mut();
            if *last != current_dock_count {
                *last = current_dock_count;
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

/// Shows the dock on the named monitor and hides it on all others.
fn show_on_monitor_only_by_name(docks: &Rc<RefCell<Vec<MonitorDock>>>, target_name: &str) {
    let dock_list = docks.borrow();
    let mut found = false;
    for dock in dock_list.iter() {
        if dock.output_name == target_name {
            dock.win.set_visible(true);
            found = true;
        } else {
            dock.win.set_visible(false);
        }
    }
    if found {
        log::debug!("Dock shown on monitor {}", target_name);
    } else {
        log::debug!("No dock window for monitor {}", target_name);
    }
}

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
