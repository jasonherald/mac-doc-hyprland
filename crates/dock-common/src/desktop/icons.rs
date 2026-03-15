use crate::config::paths::load_text_lines;
use crate::desktop::dirs::search_desktop_dirs;
use gtk4::prelude::*;
use std::path::{Path, PathBuf};

/// Resolves the icon name for an application.
///
/// Searches .desktop files in the given app directories to find the Icon= field.
pub fn get_icon(app_name: &str, app_dirs: &[PathBuf]) -> Option<String> {
    let app_name = app_name.split(' ').next().unwrap_or(app_name);

    // Special case for GIMP
    if app_name.to_uppercase().starts_with("GIMP") {
        return Some("gimp".to_string());
    }

    // Try direct match first
    let desktop_path = find_desktop_file(app_name, app_dirs)?;

    // Read the Icon= line from the .desktop file
    let lines = load_text_lines(&desktop_path).ok()?;
    for line in &lines {
        if line.to_uppercase().starts_with("ICON")
            && let Some(value) = line.split('=').nth(1)
        {
            return Some(value.to_string());
        }
    }

    None
}

/// Resolves the Exec command for an application.
pub fn get_exec(app_name: &str, app_dirs: &[PathBuf]) -> Option<String> {
    let cmd = if app_name.to_uppercase().starts_with("GIMP") {
        "gimp"
    } else {
        app_name
    };

    let desktop_path = find_desktop_file(app_name, app_dirs)?;
    let lines = load_text_lines(&desktop_path).ok()?;

    for line in &lines {
        if line.to_uppercase().starts_with("EXEC") {
            let exec = &line[5..]; // Skip "Exec="
            // Strip field codes like %u, %f, etc.
            let exec = if let Some(pos) = exec.find('%') {
                exec[..pos.saturating_sub(1)].trim()
            } else {
                exec.trim()
            };
            return Some(exec.to_string());
        }
    }

    Some(cmd.to_string())
}

/// Resolves the display name for an application.
pub fn get_name(app_name: &str, app_dirs: &[PathBuf]) -> String {
    if let Some(desktop_path) = find_desktop_file(app_name, app_dirs)
        && let Ok(lines) = load_text_lines(&desktop_path)
    {
        for line in &lines {
            if line.to_uppercase().starts_with("NAME=") {
                return line[5..].to_string();
            }
        }
    }
    app_name.to_string()
}

/// Finds a .desktop file for the given app name.
fn find_desktop_file(app_name: &str, app_dirs: &[PathBuf]) -> Option<PathBuf> {
    // Try exact match
    for dir in app_dirs {
        let path = dir.join(format!("{}.desktop", app_name));
        if path.exists() {
            return Some(path);
        }
        let lower = dir.join(format!("{}.desktop", app_name.to_lowercase()));
        if lower.exists() {
            return Some(lower);
        }
    }

    // Fall back to fuzzy search
    if !app_name.starts_with('/') {
        search_desktop_dirs(app_name, app_dirs)
    } else {
        None
    }
}

/// Creates a GTK4 pixbuf from an icon name or path.
///
/// If `icon` is an absolute path, loads from file.
/// Otherwise, tries the icon theme, then falls back to desktop file lookup.
pub fn create_pixbuf(icon: &str, size: i32) -> Option<gtk4::gdk_pixbuf::Pixbuf> {
    // Absolute path
    if icon.starts_with('/') {
        return gtk4::gdk_pixbuf::Pixbuf::from_file_at_size(icon, size, size).ok();
    }

    // Try icon theme
    let display = gtk4::gdk::Display::default()?;
    let theme = gtk4::IconTheme::for_display(&display);

    if theme.has_icon(icon) {
        // Use the icon theme's paintable, but we need a pixbuf for compatibility
        // Try loading via the theme lookup
        let icon_paintable = theme.lookup_icon(
            icon,
            &[],
            size,
            1,
            gtk4::TextDirection::None,
            gtk4::IconLookupFlags::FORCE_REGULAR,
        );
        let file = icon_paintable.file()?;
        let path = file.path()?;
        return gtk4::gdk_pixbuf::Pixbuf::from_file_at_size(path, size, size).ok();
    }

    None
}

/// Creates a GTK4 Image widget from an app ID.
pub fn create_image(app_id: &str, size: i32, app_dirs: &[PathBuf]) -> Option<gtk4::Image> {
    let icon_name = get_icon(app_id, app_dirs).unwrap_or_else(|| app_id.to_string());
    let pixbuf = create_pixbuf(&icon_name, size)?;
    Some(gtk4::Image::from_pixbuf(Some(&pixbuf)))
}

/// Loads a pixbuf from a file path at the given dimensions.
pub fn pixbuf_from_file(path: &Path, width: i32, height: i32) -> Option<gtk4::gdk_pixbuf::Pixbuf> {
    gtk4::gdk_pixbuf::Pixbuf::from_file_at_size(path, width, height).ok()
}
