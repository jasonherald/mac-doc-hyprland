//! Shared library for the nwg-dock / nwg-drawer / nwg-notifications
//! Rust ports — compositor-neutral IPC abstraction, `.desktop` entry
//! parsing, pin-file management, layer-shell helpers, and the various
//! bits of system plumbing that are common across the three tools.
//!
//! See the README for the crate's stability contract.

#![warn(missing_docs)]

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
