/// Compositor-neutral window representation.
#[derive(Debug, Clone, Default)]
pub struct WmClient {
    /// Compositor-specific identifier (Hyprland: `0x1234`, Sway: `42`).
    pub id: String,
    /// Application class (Hyprland: `class`; Sway: `app_id` or
    /// `window_properties.class`).
    pub class: String,
    /// Initial class at window creation (Hyprland only). Used to group child
    /// windows with their parent app (e.g. Playwright browsers under VSCode).
    /// Empty on backends that don't track this separately.
    pub initial_class: String,
    /// Human-readable window title.
    pub title: String,
    /// Process ID of the window's owning process, or 0 if unavailable.
    pub pid: i32,
    /// Workspace this window lives on.
    pub workspace: WmWorkspace,
    /// Whether the window is floating (not tiled).
    pub floating: bool,
    /// ID of the monitor this window is on. Matches [`WmMonitor::id`].
    pub monitor_id: i32,
    /// Whether the window is currently fullscreen.
    pub fullscreen: bool,
}

/// Compositor-neutral monitor/output.
#[derive(Debug, Clone, Default)]
pub struct WmMonitor {
    /// Compositor-assigned numeric ID. Matches [`WmClient::monitor_id`].
    pub id: i32,
    /// Output connector name (e.g. `DP-1`, `eDP-1`).
    pub name: String,
    /// Physical pixel width.
    pub width: i32,
    /// Physical pixel height.
    pub height: i32,
    /// Global x-offset in the compositor's layout.
    pub x: i32,
    /// Global y-offset in the compositor's layout.
    pub y: i32,
    /// HiDPI scale factor (1.0 = no scaling).
    pub scale: f64,
    /// Whether this monitor currently holds keyboard focus.
    pub focused: bool,
    /// Workspace that's active on this monitor.
    pub active_workspace: WmWorkspace,
}

/// Compositor-neutral workspace reference.
#[derive(Debug, Clone, Default)]
pub struct WmWorkspace {
    /// Compositor-assigned numeric workspace ID.
    pub id: i32,
    /// Workspace name (may be numeric-as-string or a human name like `chat`).
    pub name: String,
}

/// Events from the compositor event stream.
#[derive(Debug, Clone)]
pub enum WmEvent {
    /// Active window changed. Contains the window id.
    ActiveWindowChanged(String),
    /// Monitor added or removed (hotplug).
    MonitorChanged,
    /// Any other event.
    Other(String),
}
