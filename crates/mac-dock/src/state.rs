use dock_common::compositor::{Compositor, WmClient, WmMonitor};
use std::path::PathBuf;
use std::rc::Rc;

/// Window/monitor tracking state.
pub struct DockState {
    pub clients: Vec<WmClient>,
    pub active_client: Option<WmClient>,
    pub monitors: Vec<WmMonitor>,
    pub pinned: Vec<String>,
    pub app_dirs: Vec<PathBuf>,

    /// Compositor backend for IPC calls.
    pub compositor: Rc<dyn Compositor>,

    /// Scaled icon size (adjusted when many apps are open).
    pub img_size_scaled: i32,

    /// Last window id from event stream (used for change detection).
    pub last_win_addr: String,

    /// True when a popover menu is open — prevents autohide.
    pub popover_open: bool,

    /// True when dock arrangement is locked (drag-to-reorder disabled).
    pub locked: bool,

    /// Index of the pinned item currently being dragged (if any).
    pub drag_source_index: Option<usize>,

    /// True when a drag is active and cursor is outside the dock area.
    /// Used to show a "remove" indicator on the dragged item's slot.
    pub drag_outside_dock: bool,
}

impl DockState {
    pub fn new(app_dirs: Vec<PathBuf>, compositor: Rc<dyn Compositor>) -> Self {
        Self {
            clients: Vec::new(),
            active_client: None,
            monitors: Vec::new(),
            pinned: Vec::new(),
            app_dirs,
            compositor,
            img_size_scaled: 48,
            last_win_addr: String::new(),
            popover_open: false,
            locked: false,
            drag_source_index: None,
            drag_outside_dock: false,
        }
    }

    /// Finds all client instances matching a class (case-insensitive).
    pub fn task_instances(&self, class: &str) -> Vec<WmClient> {
        self.clients
            .iter()
            .filter(|c| c.class.eq_ignore_ascii_case(class))
            .cloned()
            .collect()
    }

    /// Refreshes client list from the compositor.
    pub fn refresh_clients(&mut self) -> anyhow::Result<()> {
        self.clients = self.compositor.list_clients()?;
        self.active_client = self.compositor.get_active_window().ok();
        Ok(())
    }

    /// Refreshes monitor list from the compositor.
    pub fn refresh_monitors(&mut self) -> anyhow::Result<()> {
        self.monitors = self.compositor.list_monitors()?;
        Ok(())
    }
}
