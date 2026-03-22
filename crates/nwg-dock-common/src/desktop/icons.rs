use crate::desktop::dirs::search_desktop_dirs;
use crate::desktop::entry::parse_desktop_file;
use gtk4::prelude::*;
use std::path::{Path, PathBuf};

/// Resolves the icon name for an application.
///
/// Searches .desktop files in the given app directories to find the Icon field.
pub fn get_icon(app_name: &str, app_dirs: &[PathBuf]) -> Option<String> {
    let app_name = app_name.split(' ').next().unwrap_or(app_name);

    // Special case for GIMP
    if app_name.to_uppercase().starts_with("GIMP") {
        return Some("gimp".to_string());
    }

    let desktop_path = find_desktop_file(app_name, app_dirs)?;
    let entry = parse_desktop_file(app_name, &desktop_path).ok()?;
    if entry.icon.is_empty() {
        None
    } else {
        Some(entry.icon)
    }
}

/// Resolves the Exec command for an application.
pub fn get_exec(app_name: &str, app_dirs: &[PathBuf]) -> Option<String> {
    let cmd = if app_name.to_uppercase().starts_with("GIMP") {
        "gimp"
    } else {
        app_name
    };

    let desktop_path = find_desktop_file(app_name, app_dirs)?;
    let entry = parse_desktop_file(app_name, &desktop_path).ok()?;

    if entry.exec.is_empty() {
        return Some(cmd.to_string());
    }

    Some(super::entry::strip_field_codes(&entry.exec))
}

/// Resolves the display name for an application.
///
/// Returns the locale-aware name (e.g., Name[pl] for Polish) if available,
/// falling back to the base Name field, then the raw app class name.
/// Fixes: nwg-piotr/nwg-dock#56 (broken application names)
pub fn get_name(app_name: &str, app_dirs: &[PathBuf]) -> String {
    if let Some(desktop_path) = find_desktop_file(app_name, app_dirs)
        && let Ok(entry) = parse_desktop_file(app_name, &desktop_path)
    {
        if !entry.name_loc.is_empty() {
            return entry.name_loc;
        }
        if !entry.name.is_empty() {
            return entry.name;
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
    if icon.contains('/') {
        return gtk4::gdk_pixbuf::Pixbuf::from_file_at_size(icon, size, size).ok();
    }

    // Strip file extensions (matches Go behavior for entries like "Icon=firefox.svg")
    let icon_name = icon
        .strip_suffix(".svg")
        .or_else(|| icon.strip_suffix(".png"))
        .or_else(|| icon.strip_suffix(".xpm"))
        .unwrap_or(icon);

    // Try icon theme
    let display = gtk4::gdk::Display::default()?;
    let theme = gtk4::IconTheme::for_display(&display);

    if theme.has_icon(icon_name) {
        let icon_paintable = theme.lookup_icon(
            icon_name,
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

    // Fallback: try original name (with extension) in case it's a custom theme icon
    if icon_name != icon && theme.has_icon(icon) {
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

    // Fallback: try /usr/share/pixmaps (many apps install icons there)
    for ext in &["svg", "png", "xpm"] {
        let path =
            std::path::Path::new("/usr/share/pixmaps").join(format!("{}.{}", icon_name, ext));
        if path.exists() {
            return gtk4::gdk_pixbuf::Pixbuf::from_file_at_size(path, size, size).ok();
        }
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
