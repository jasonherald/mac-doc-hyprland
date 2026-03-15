use std::path::PathBuf;

/// Unified error type for dock-common operations.
#[derive(Debug, thiserror::Error)]
pub enum DockError {
    #[error("Hyprland IPC error: {0}")]
    HyprlandIpc(#[from] std::io::Error),

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
}

pub type Result<T> = std::result::Result<T, DockError>;
