pub mod compositor;
pub mod config;
pub mod desktop;
pub mod launch;
pub mod layer_shell;
pub mod pinning;
pub mod process;
pub mod signals;
pub mod singleton;

mod error;
mod hyprland;

pub use error::{DockError, Result};
