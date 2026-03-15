use dock_common::hyprland::types::{HyprClient, HyprMonitor};
use std::path::PathBuf;

/// Hyprland client/window tracking state.
pub struct DockState {
    pub clients: Vec<HyprClient>,
    pub active_client: Option<HyprClient>,
    pub monitors: Vec<HyprMonitor>,
    pub pinned: Vec<String>,
    pub app_dirs: Vec<PathBuf>,

    /// Scaled icon size (adjusted when many apps are open).
    pub img_size_scaled: i32,

    /// Last window address from socket2 events (used for change detection).
    pub last_win_addr: String,

    /// True when a popover menu is open — prevents autohide.
    pub popover_open: bool,
}

impl DockState {
    pub fn new(app_dirs: Vec<PathBuf>) -> Self {
        Self {
            clients: Vec::new(),
            active_client: None,
            monitors: Vec::new(),
            pinned: Vec::new(),
            app_dirs,
            img_size_scaled: 48,
            last_win_addr: String::new(),
            popover_open: false,
        }
    }

    /// Finds all client instances matching a class (case-insensitive).
    pub fn task_instances(&self, class: &str) -> Vec<HyprClient> {
        self.clients
            .iter()
            .filter(|c| c.class.eq_ignore_ascii_case(class))
            .cloned()
            .collect()
    }

    /// Refreshes client list from Hyprland.
    pub fn refresh_clients(&mut self) -> anyhow::Result<()> {
        self.clients = dock_common::hyprland::ipc::list_clients()?;
        self.active_client = dock_common::hyprland::ipc::get_active_window().ok();
        Ok(())
    }

    /// Refreshes monitor list from Hyprland.
    pub fn refresh_monitors(&mut self) -> anyhow::Result<()> {
        self.monitors = dock_common::hyprland::ipc::list_monitors()?;
        Ok(())
    }
}
