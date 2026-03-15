use dock_common::desktop::icons;
use std::path::PathBuf;

/// Resolves the best icon for a notification.
///
/// For popup windows, uses `create_pixbuf` for high-quality rendering.
/// Falls through: app_icon → desktop_entry → app_name → fallback.
pub fn resolve_popup_icon(
    app_icon: &str,
    app_name: &str,
    desktop_entry: Option<&str>,
    app_dirs: &[PathBuf],
    size: i32,
) -> gtk4::Image {
    if !app_icon.is_empty()
        && let Some(pb) = icons::create_pixbuf(app_icon, size)
    {
        return gtk4::Image::from_pixbuf(Some(&pb));
    }

    if let Some(entry) = desktop_entry
        && let Some(icon_name) = icons::get_icon(entry, app_dirs)
        && let Some(pb) = icons::create_pixbuf(&icon_name, size)
    {
        return gtk4::Image::from_pixbuf(Some(&pb));
    }

    if let Some(icon_name) = icons::get_icon(app_name, app_dirs)
        && let Some(pb) = icons::create_pixbuf(&icon_name, size)
    {
        return gtk4::Image::from_pixbuf(Some(&pb));
    }

    let img = gtk4::Image::from_icon_name("dialog-information");
    img.set_pixel_size(size);
    img
}

/// Resolves an icon using GTK4 icon theme names only.
///
/// Used in the panel where glycin pixbuf loading can cause crashes
/// during rapid rebuilds. Falls through: app_icon → app_name → fallback.
pub fn resolve_theme_icon(
    app_icon: &str,
    app_name: &str,
    app_dirs: &[PathBuf],
    size: i32,
) -> gtk4::Image {
    if !app_icon.is_empty() && !app_icon.contains('/') {
        let img = gtk4::Image::from_icon_name(app_icon);
        img.set_pixel_size(size);
        return img;
    }

    if let Some(icon_name) = icons::get_icon(app_name, app_dirs)
        && !icon_name.contains('/')
    {
        let img = gtk4::Image::from_icon_name(&icon_name);
        img.set_pixel_size(size);
        return img;
    }

    let img = gtk4::Image::from_icon_name("dialog-information");
    img.set_pixel_size(size);
    img
}
