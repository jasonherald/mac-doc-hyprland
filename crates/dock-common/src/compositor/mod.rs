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
}
