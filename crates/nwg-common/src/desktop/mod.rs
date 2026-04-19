//! `.desktop` file parsing and the surrounding ecosystem: application
//! directories, icon resolution, FreeDesktop category assignment, and
//! user-configured preferred-app overrides.

/// FreeDesktop category resolution and the drawer's default category taxonomy.
pub mod categories;

/// Resolution of `XDG_DATA_DIRS` application directories.
pub mod dirs;

/// `.desktop` file parsing.
pub mod entry;

/// Icon file lookup + display-name resolution.
pub mod icons;

/// User-configured `mime-type → desktop-id` preferred-app overrides.
pub mod preferred_apps;
