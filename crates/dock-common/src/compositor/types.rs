/// Compositor-neutral window representation.
#[derive(Debug, Clone, Default)]
pub struct WmClient {
    /// Compositor-specific identifier (Hyprland: "0x1234", Sway: "42").
    pub id: String,
    /// Application class (Hyprland: class, Sway: app_id or window_properties.class).
    pub class: String,
    pub title: String,
    pub pid: i32,
    pub workspace: WmWorkspace,
    pub floating: bool,
    pub monitor_id: i32,
    pub fullscreen: bool,
}

/// Compositor-neutral monitor/output.
#[derive(Debug, Clone, Default)]
pub struct WmMonitor {
    pub id: i32,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub focused: bool,
    pub active_workspace: WmWorkspace,
}

/// Compositor-neutral workspace reference.
#[derive(Debug, Clone, Default)]
pub struct WmWorkspace {
    pub id: i32,
    pub name: String,
}

/// Events from the compositor event stream.
#[derive(Debug, Clone)]
pub enum WmEvent {
    /// Active window changed. Contains the window id.
    ActiveWindowChanged(String),
    /// Any other event.
    Other(String),
}
