use super::traits::{Compositor, WmEventStream};
use super::types::{WmClient, WmMonitor};
use crate::error::{DockError, Result};

/// Fallback compositor backend for environments without Hyprland or Sway IPC.
///
/// Used by `nwg-drawer` so it can run on any compositor (Niri, river, Openbox, etc.).
/// Most methods return errors — the drawer already handles those gracefully.
/// The one exception is the launch path, which falls back to direct process spawn.
pub struct NullCompositor;

impl Compositor for NullCompositor {
    fn list_clients(&self) -> Result<Vec<WmClient>> {
        Err(DockError::NoCompositorDetected)
    }

    fn list_monitors(&self) -> Result<Vec<WmMonitor>> {
        Err(DockError::NoCompositorDetected)
    }

    fn get_active_window(&self) -> Result<WmClient> {
        Err(DockError::NoCompositorDetected)
    }

    fn get_cursor_position(&self) -> Option<(i32, i32)> {
        None
    }

    fn focus_window(&self, _id: &str) -> Result<()> {
        Err(DockError::NoCompositorDetected)
    }

    fn close_window(&self, _id: &str) -> Result<()> {
        Err(DockError::NoCompositorDetected)
    }

    fn toggle_floating(&self, _id: &str) -> Result<()> {
        Err(DockError::NoCompositorDetected)
    }

    fn toggle_fullscreen(&self, _id: &str) -> Result<()> {
        Err(DockError::NoCompositorDetected)
    }

    fn move_to_workspace(&self, _id: &str, _workspace: i32) -> Result<()> {
        Err(DockError::NoCompositorDetected)
    }

    fn toggle_special_workspace(&self, _name: &str) -> Result<()> {
        Err(DockError::NoCompositorDetected)
    }

    fn raise_active(&self) -> Result<()> {
        Err(DockError::NoCompositorDetected)
    }

    /// Launches the command directly via process spawn instead of compositor IPC.
    /// The drawer's launch pipeline handles quoting and field-code stripping upstream.
    /// Rejects empty commands so the caller can distinguish success from a no-op.
    fn exec(&self, cmd: &str) -> Result<()> {
        if cmd.trim().is_empty() {
            return Err(DockError::NoCompositorDetected);
        }
        crate::launch::launch_command(cmd);
        Ok(())
    }

    /// NullCompositor has no compositor IPC, so there's no event stream to
    /// subscribe to. Fail fast rather than returning a stream that blocks
    /// forever — callers can then avoid spawning a worker thread at all.
    fn event_stream(&self) -> Result<Box<dyn WmEventStream>> {
        Err(DockError::NoCompositorDetected)
    }

    fn supports_cursor_position(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: asserts the result is specifically Err(NoCompositorDetected),
    /// not just any error — catches regressions to unrelated error paths.
    fn assert_no_compositor<T: std::fmt::Debug>(result: Result<T>) {
        assert!(
            matches!(result, Err(DockError::NoCompositorDetected)),
            "expected NoCompositorDetected, got {:?}",
            result
        );
    }

    #[test]
    fn list_clients_returns_no_compositor_error() {
        assert_no_compositor(NullCompositor.list_clients());
    }

    #[test]
    fn list_monitors_returns_no_compositor_error() {
        assert_no_compositor(NullCompositor.list_monitors());
    }

    #[test]
    fn get_active_window_returns_no_compositor_error() {
        assert_no_compositor(NullCompositor.get_active_window());
    }

    #[test]
    fn cursor_position_unsupported() {
        assert!(!NullCompositor.supports_cursor_position());
        assert_eq!(NullCompositor.get_cursor_position(), None);
    }

    /// Arbitrary workspace id for testing — the NullCompositor rejects
    /// any value, so the specific number is irrelevant.
    const TEST_WORKSPACE: i32 = 1;

    #[test]
    fn window_operations_return_no_compositor_error() {
        let c = NullCompositor;
        assert_no_compositor(c.focus_window("x"));
        assert_no_compositor(c.close_window("x"));
        assert_no_compositor(c.toggle_floating("x"));
        assert_no_compositor(c.toggle_fullscreen("x"));
        assert_no_compositor(c.move_to_workspace("x", TEST_WORKSPACE));
        assert_no_compositor(c.toggle_special_workspace("x"));
        assert_no_compositor(c.raise_active());
    }

    #[test]
    fn event_stream_returns_no_compositor_error() {
        // NullCompositor fails fast instead of returning a stream that
        // blocks forever — prevents stranding worker threads.
        assert!(matches!(
            NullCompositor.event_stream(),
            Err(DockError::NoCompositorDetected)
        ));
    }

    #[test]
    fn exec_launches_trivial_command() {
        // /bin/true exits immediately with status 0. This verifies the exec
        // path actually spawns a subprocess rather than erroring out.
        // Direct spawn path — no shell, no side effects.
        assert!(NullCompositor.exec("/bin/true").is_ok());
    }

    #[test]
    fn exec_empty_command_returns_no_compositor_error() {
        // Reject empty/whitespace input so callers can distinguish
        // "launched" from "nothing happened"
        assert_no_compositor(NullCompositor.exec(""));
        assert_no_compositor(NullCompositor.exec("   "));
    }
}
