//! Configuration helpers: CSS loading and hot-reload, CLI flag normalization,
//! and XDG path resolution.

/// CSS loading, watching, and `@import` graph resolution for GTK4 providers.
pub mod css;

/// CLI flag normalization for legacy single-dash forms (e.g. `-d` → `--daemon`).
pub mod flags;

/// XDG data/config/cache directory resolution.
pub mod paths;
