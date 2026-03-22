mod cursor_poller;
mod hotspot_windows;

use crate::config::DockConfig;
use crate::dock_windows::MonitorDock;
use crate::state::DockState;
use nwg_dock_common::compositor::Compositor;
use std::cell::RefCell;
use std::rc::Rc;

pub use hotspot_windows::HotspotContext;

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
        cursor_poller::start_cursor_poller(per_monitor, config, state, compositor);
        None
    } else {
        Some(hotspot_windows::start_hotspot_windows(
            per_monitor,
            config,
            state,
            app,
        ))
    }
}
