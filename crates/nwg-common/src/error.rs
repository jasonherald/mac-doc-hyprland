use std::path::PathBuf;

/// Unified error type for dock-common operations.
#[derive(Debug, thiserror::Error)]
pub enum DockError {
    #[error("compositor IPC error: {0}")]
    Ipc(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("desktop entry parse error in {path}: {message}")]
    DesktopEntry { path: PathBuf, message: String },

    #[error("icon not found for '{0}'")]
    IconNotFound(String),

    #[error("data directory not found for '{0}'")]
    DataDirNotFound(String),

    #[error("lock file already held: {path} (pid {pid})")]
    LockFileHeld { path: PathBuf, pid: u32 },

    #[error("environment variable not set: {0}")]
    EnvNotSet(String),

    #[error("unsupported compositor: {0}")]
    UnsupportedCompositor(String),

    #[error("no compositor detected (set HYPRLAND_INSTANCE_SIGNATURE or SWAYSOCK)")]
    NoCompositorDetected,
}

pub type Result<T> = std::result::Result<T, DockError>;
