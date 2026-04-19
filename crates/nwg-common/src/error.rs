use std::path::PathBuf;

/// Unified error type for `nwg-common` fallible operations.
///
/// Re-exported at the crate root as `nwg_common::DockError` so consumers
/// can pattern-match on errors returned by the [`Compositor`](crate::compositor::Compositor)
/// trait and other public APIs without reaching into an `error` submodule.
#[derive(Debug, thiserror::Error)]
pub enum DockError {
    /// Underlying compositor IPC I/O failure (socket connect, read, write).
    #[error("compositor IPC error: {0}")]
    Ipc(#[from] std::io::Error),

    /// The JSON returned by a compositor IPC query didn't parse into the
    /// expected type.
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// A `.desktop` file was malformed.
    #[error("desktop entry parse error in {path}: {message}")]
    DesktopEntry {
        /// Path of the offending file.
        path: PathBuf,
        /// Human-readable description of the parse failure.
        message: String,
    },

    /// Icon lookup failed — no file matched the requested icon name.
    #[error("icon not found for '{0}'")]
    IconNotFound(String),

    /// An expected data directory (XDG or install-time) wasn't found.
    #[error("data directory not found for '{0}'")]
    DataDirNotFound(String),

    /// Single-instance lock file is already held by another live process.
    #[error("lock file already held: {path} (pid {pid})")]
    LockFileHeld {
        /// Path to the lock file.
        path: PathBuf,
        /// PID of the instance currently holding it.
        pid: u32,
    },

    /// A required environment variable was unset — e.g. `HYPRLAND_INSTANCE_SIGNATURE`
    /// on a Hyprland session.
    #[error("environment variable not set: {0}")]
    EnvNotSet(String),

    /// A `--wm` override specified a backend this library doesn't implement.
    #[error("unsupported compositor: {0}")]
    UnsupportedCompositor(String),

    /// Neither Hyprland nor Sway env vars are set, and no override was given.
    #[error("no compositor detected (set HYPRLAND_INSTANCE_SIGNATURE or SWAYSOCK)")]
    NoCompositorDetected,
}

/// Shorthand for `std::result::Result<T, DockError>`.
pub type Result<T> = std::result::Result<T, DockError>;
