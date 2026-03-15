use dock_common::hyprland::types::{HyprClient, HyprMonitor};
use std::path::PathBuf;

/// Mutable state for the dock application.
#[allow(dead_code)]
pub struct DockState {
    pub clients: Vec<HyprClient>,
    pub old_clients: Vec<HyprClient>,
    pub active_client: Option<HyprClient>,
    pub monitors: Vec<HyprMonitor>,
    pub pinned: Vec<String>,
    pub app_dirs: Vec<PathBuf>,

    /// Scaled icon size (adjusted when many apps are open)
    pub img_size_scaled: i32,

    /// Last window address seen from socket2 events
    pub last_win_addr: String,

    /// GLib source handle for the autohide timeout
    pub close_timeout_src: u32,

    /// Track mouse position for autohide
    pub mouse_inside_dock: bool,
    pub mouse_inside_hotspot: bool,

    /// Timestamp when detector was entered (for hotspot delay)
    pub detector_entered_at: i64,
}

impl DockState {
    pub fn new(app_dirs: Vec<PathBuf>) -> Self {
        Self {
            clients: Vec::new(),
            old_clients: Vec::new(),
            active_client: None,
            monitors: Vec::new(),
            pinned: Vec::new(),
            app_dirs,
            img_size_scaled: 48,
            last_win_addr: String::new(),
            close_timeout_src: 0,
            mouse_inside_dock: false,
            mouse_inside_hotspot: false,
            detector_entered_at: 0,
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

    /// Whether a class is currently running as a task.
    #[allow(dead_code)]
    pub fn in_tasks(&self, class: &str) -> bool {
        self.clients
            .iter()
            .any(|c| c.class.trim() == class.trim())
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
