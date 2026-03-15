use dock_common::desktop::icons;
use gtk4::prelude::*;

/// Creates a GTK4 button with icon above label, matching macOS Launchpad style.
///
/// Shared between app_grid and pinned modules to eliminate duplication.
pub fn app_icon_button(
    icon_name: &str,
    display_name: &str,
    icon_size: i32,
    app_dirs: &[std::path::PathBuf],
) -> gtk4::Button {
    let button = gtk4::Button::new();
    button.set_has_frame(false);
    button.add_css_class("app-button");

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    vbox.set_halign(gtk4::Align::Center);

    // Icon
    if !icon_name.is_empty()
        && let Some(image) = icons::create_image(icon_name, icon_size, app_dirs)
    {
        image.set_pixel_size(icon_size);
        image.set_halign(gtk4::Align::Center);
        vbox.append(&image);
    }

    // Label
    let label = gtk4::Label::new(Some(&truncate(display_name, 20)));
    label.set_halign(gtk4::Align::Center);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_max_width_chars(14);
    vbox.append(&label);

    button.set_child(Some(&vbox));
    button
}

/// Strips field codes (%u, %f, etc.) and quotes from an Exec command.
pub fn clean_exec(exec: &str) -> String {
    let exec = exec.replace(['"', '\''], "");
    if let Some(pos) = exec.find('%') {
        exec[..pos.saturating_sub(1)].trim().to_string()
    } else {
        exec.trim().to_string()
    }
}

/// Truncates a string to max chars, appending ellipsis if needed.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_exec_strips_field_codes() {
        assert_eq!(clean_exec("firefox %u"), "firefox");
        assert_eq!(clean_exec("code %F"), "code");
        assert_eq!(clean_exec("gimp"), "gimp");
    }

    #[test]
    fn clean_exec_strips_quotes() {
        assert_eq!(clean_exec(r#""firefox" %u"#), "firefox");
        assert_eq!(clean_exec("'brave' --new-window"), "brave --new-window");
    }

    #[test]
    fn clean_exec_trims_whitespace() {
        assert_eq!(clean_exec("  firefox  "), "firefox");
        assert_eq!(clean_exec(""), "");
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("Hi", 20), "Hi");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("Very Long Application Name Here", 10);
        assert!(result.ends_with('…'));
        assert!(result.chars().count() <= 10);
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("12345", 5), "12345");
    }

    #[test]
    fn truncate_unicode() {
        // Ensure char-based truncation, not byte-based
        let result = truncate("日本語のアプリケーション名前テスト", 5);
        assert!(result.ends_with('…'));
        assert_eq!(result.chars().count(), 5);
    }
}
