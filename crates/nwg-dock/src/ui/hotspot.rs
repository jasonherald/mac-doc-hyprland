use crate::config::DockConfig;
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

/// Sets up autohide using the appropriate method for the compositor.
///
/// - Compositors with cursor position IPC (Hyprland): poll cursor position
/// - Compositors without (Sway): use thin GTK layer-shell hotspot windows
pub fn setup_autohide(
    dock_windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    compositor: &Rc<dyn Compositor>,
    app: &gtk4::Application,
    monitors: &[gtk4::gdk::Monitor],
) {
    if compositor.supports_cursor_position() {
        start_cursor_poller(dock_windows, config, state, compositor);
    } else {
        start_hotspot_windows(dock_windows, config, state, app, monitors);
    }
}

// =============================================================================
// GTK hotspot approach (for Sway and other compositors without cursor IPC)
// =============================================================================

/// Creates thin layer-shell windows at the dock edge to trigger show on hover.
/// Uses GTK4 EventControllerMotion for enter/leave detection.
fn start_hotspot_windows(
    dock_windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    app: &gtk4::Application,
    monitors: &[gtk4::gdk::Monitor],
) {
    let hide_timeout = config.hide_timeout;
    let position = config.position;

    // Shared hide timer state
    let left_at: Rc<RefCell<Option<std::time::Instant>>> = Rc::new(RefCell::new(None));

    for (i, mon) in monitors.iter().enumerate() {
        create_hotspot_window(app, position, mon, i, dock_windows, &left_at);
    }

    // Poll the hide timer to actually hide dock windows
    let dock_windows = Rc::clone(dock_windows);
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
                for win in dock_windows.borrow().iter() {
                    win.set_visible(false);
                }
                *left = None;
            }
        }
        glib::ControlFlow::Continue
    });
}

/// Creates a single hotspot trigger window for one monitor and attaches enter/leave handlers.
fn create_hotspot_window(
    app: &gtk4::Application,
    position: crate::config::Position,
    mon: &gtk4::gdk::Monitor,
    monitor_index: usize,
    dock_windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    left_at: &Rc<RefCell<Option<std::time::Instant>>>,
) {
    let dock_windows = Rc::clone(dock_windows);

    // --- Create the hotspot trigger window ---
    let hotspot = gtk4::ApplicationWindow::new(app);
    hotspot.init_layer_shell();
    hotspot.set_namespace(Some("nwg-dock-hotspot"));
    setup_hotspot_layer(&hotspot, position);
    hotspot.set_monitor(Some(mon));

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

    // Hotspot enter → show dock on this monitor
    let dock_wins_enter = Rc::clone(&dock_windows);
    let left_at_enter = Rc::clone(left_at);
    let motion = gtk4::EventControllerMotion::new();
    motion.connect_enter(move |_, _, _| {
        let wins = dock_wins_enter.borrow();
        if monitor_index < wins.len() {
            log::debug!("Hotspot entered, showing dock on monitor {}", monitor_index);
            wins[monitor_index].set_visible(true);
        }
        *left_at_enter.borrow_mut() = None;
    });
    hotspot.add_controller(motion);

    // --- Attach enter/leave to the dock window ---
    let wins = dock_windows.borrow();
    if monitor_index < wins.len() {
        let dock_win = &wins[monitor_index];

        // Dock enter → cancel hide timer
        let left_at_dock_enter = Rc::clone(left_at);
        let dock_motion = gtk4::EventControllerMotion::new();
        dock_motion.connect_enter(move |_, _, _| {
            *left_at_dock_enter.borrow_mut() = None;
        });
        dock_win.add_controller(dock_motion);

        // Dock leave → start hide timer
        let left_at_dock_leave = Rc::clone(left_at);
        let leave_motion = gtk4::EventControllerMotion::new();
        leave_motion.connect_leave(move |_| {
            *left_at_dock_leave.borrow_mut() = Some(std::time::Instant::now());
        });
        dock_win.add_controller(leave_motion);
    }
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
fn start_cursor_poller(
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
            handle_hidden_dock(&cursor, &monitors, position, &windows, &left_at);
        } else {
            handle_visible_dock(
                &cursor,
                &monitors,
                position,
                &windows,
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
    windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    left_at: &Rc<RefCell<Option<std::time::Instant>>>,
) {
    if is_cursor_at_edge(cursor, monitors, position) {
        if let Some(mon_idx) = find_cursor_monitor(cursor, monitors) {
            let wins = windows.borrow();
            if mon_idx < wins.len() {
                log::debug!("Cursor at edge, showing dock on monitor {}", mon_idx);
                wins[mon_idx].set_visible(true);
            }
        }
        *left_at.borrow_mut() = None;
    }
}

/// Handles cursor polling when the dock is visible: hides after timeout if cursor leaves.
fn handle_visible_dock(
    cursor: &CursorPos,
    monitors: &[WmMonitor],
    position: crate::config::Position,
    windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    state: &Rc<RefCell<DockState>>,
    left_at: &Rc<RefCell<Option<std::time::Instant>>>,
    hide_timeout: u64,
) {
    let in_dock_area = is_cursor_in_visible_dock(cursor, windows, monitors, position);
    let at_edge = is_cursor_at_edge(cursor, monitors, position);

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

/// Checks if the cursor is within the bounds of any visible dock window.
/// Uses the window's allocated size and monitor positions to compute bounds.
fn is_cursor_in_visible_dock(
    cursor: &CursorPos,
    windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    monitors: &[WmMonitor],
    position: crate::config::Position,
) -> bool {
    let wins = windows.borrow();
    for win in wins.iter() {
        if !win.is_visible() || win.surface().is_none() {
            continue;
        }
        let w = win.width();
        let h = win.height();
        if w == 0 || h == 0 {
            continue;
        }
        if cursor_in_any_monitor_bounds(cursor, monitors, w, h, position) {
            return true;
        }
    }
    false
}

/// Returns true if the cursor falls within the dock bounds on any monitor.
fn cursor_in_any_monitor_bounds(
    cursor: &CursorPos,
    monitors: &[WmMonitor],
    w: i32,
    h: i32,
    position: crate::config::Position,
) -> bool {
    monitors.iter().any(|mon| {
        let (dock_x, dock_y) = dock_bounds_for_position(mon, w, h, position);
        cursor.x >= dock_x && cursor.x < dock_x + w && cursor.y >= dock_y && cursor.y < dock_y + h
    })
}
