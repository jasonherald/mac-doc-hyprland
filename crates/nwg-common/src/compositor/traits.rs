use crate::error::Result;

use super::types::{WmClient, WmEvent, WmMonitor};

/// Abstraction over window manager IPC (Hyprland, Sway, etc.).
///
/// Implemented once per compositor. Created at startup and shared via `Rc<dyn Compositor>`.
pub trait Compositor {
    /// List all windows/clients.
    fn list_clients(&self) -> Result<Vec<WmClient>>;

    /// List all monitors/outputs.
    fn list_monitors(&self) -> Result<Vec<WmMonitor>>;

    /// Get the currently focused window.
    fn get_active_window(&self) -> Result<WmClient>;

    /// Get cursor position. Returns `None` if the compositor doesn't expose this.
    fn get_cursor_position(&self) -> Option<(i32, i32)>;

    /// Focus a window by its compositor-specific ID.
    fn focus_window(&self, id: &str) -> Result<()>;

    /// Close a window by its compositor-specific ID.
    fn close_window(&self, id: &str) -> Result<()>;

    /// Toggle floating state of a window.
    fn toggle_floating(&self, id: &str) -> Result<()>;

    /// Toggle fullscreen state of a window.
    fn toggle_fullscreen(&self, id: &str) -> Result<()>;

    /// Move a window to a workspace.
    fn move_to_workspace(&self, id: &str, workspace: i32) -> Result<()>;

    /// Toggle a special/scratchpad workspace.
    /// Hyprland: toggles the named special workspace. Sway: toggles the scratchpad.
    fn toggle_special_workspace(&self, name: &str) -> Result<()>;

    /// Raise the active window to the top of the stack.
    /// Hyprland: `bringactivetotop`. Sway: no-op (manages its own stacking).
    fn raise_active(&self) -> Result<()>;

    /// Launch a command via the compositor's exec mechanism.
    fn exec(&self, cmd: &str) -> Result<()>;

    /// Connect to the compositor's event stream.
    fn event_stream(&self) -> Result<Box<dyn WmEventStream>>;

    /// Whether this compositor supports cursor position queries.
    fn supports_cursor_position(&self) -> bool;
}

/// Blocking event stream reader. Designed to be used from a background thread.
pub trait WmEventStream: Send {
    /// Blocks until the next event. Returns `Err` on connection failure.
    fn next_event(&mut self) -> std::result::Result<WmEvent, std::io::Error>;
}
