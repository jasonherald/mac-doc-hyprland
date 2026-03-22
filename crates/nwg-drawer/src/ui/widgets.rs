use super::constants;
use gtk4::prelude::*;
use nwg_dock_common::desktop::icons;

/// Creates a GTK4 button with icon above label, matching macOS Launchpad style.
///
/// Shared between app_grid and pinned modules to eliminate duplication.
/// Creates a GTK4 button with icon above label, matching macOS Launchpad style.
///
/// If `status_label` and `description` are provided, the button updates the
/// status bar on hover/focus with the app description (matching Go behavior).
pub fn app_icon_button(
    icon_name: &str,
    display_name: &str,
    icon_size: i32,
    app_dirs: &[std::path::PathBuf],
    status_label: &gtk4::Label,
    description: &str,
) -> gtk4::Button {
    let button = gtk4::Button::new();
    button.set_has_frame(false);
    button.add_css_class("app-button");

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    vbox.set_halign(gtk4::Align::Center);

    // Icon — try theme/file, fall back to generic "application-x-executable"
    let image = if !icon_name.is_empty() {
        icons::create_image(icon_name, icon_size, app_dirs)
    } else {
        None
    };
    let image = image.unwrap_or_else(|| {
        let fallback = gtk4::Image::from_icon_name("application-x-executable");
        fallback.set_pixel_size(icon_size);
        fallback
    });
    image.set_pixel_size(icon_size);
    image.set_halign(gtk4::Align::Center);
    vbox.append(&image);

    // Label
    let label = gtk4::Label::new(Some(&truncate(display_name, constants::APP_NAME_MAX_CHARS)));
    label.set_halign(gtk4::Align::Center);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_max_width_chars(constants::APP_LABEL_MAX_WIDTH_CHARS);
    vbox.append(&label);

    button.set_child(Some(&vbox));

    // Status label: show description on hover/focus, clear on leave
    if !description.is_empty() {
        let desc = description.to_string();
        let label_enter = status_label.clone();
        let motion = gtk4::EventControllerMotion::new();
        let desc_enter = desc.clone();
        motion.connect_enter(move |_, _, _| {
            label_enter.set_text(&desc_enter);
        });
        let label_leave = status_label.clone();
        motion.connect_leave(move |_| {
            label_leave.set_text("");
        });
        button.add_controller(motion);

        // Also update on keyboard focus
        let label_focus = status_label.clone();
        let focus_ctrl = gtk4::EventControllerFocus::new();
        focus_ctrl.connect_enter(move |_| {
            label_focus.set_text(&desc);
        });
        let label_unfocus = status_label.clone();
        focus_ctrl.connect_leave(move |_| {
            label_unfocus.set_text("");
        });
        button.add_controller(focus_ctrl);
    }

    button
}

/// Adds a pin indicator dot to the left of the app label.
///
/// Finds the Label inside the button's VBox, removes it, wraps it in a
/// horizontal Box with a small dot + label, and re-appends it to the VBox.
pub fn apply_pin_badge(button: &gtk4::Button) {
    let Some(vbox_widget) = button.child() else {
        return;
    };
    let Ok(vbox) = vbox_widget.downcast::<gtk4::Box>() else {
        return;
    };

    // Find the label (second child after Image)
    let Some(image) = vbox.first_child() else {
        return;
    };
    let Some(label_widget) = image.next_sibling() else {
        return;
    };

    // Remove label from vbox
    vbox.remove(&label_widget);

    // Create horizontal box: [dot] [label]
    let hbox = gtk4::Box::new(
        gtk4::Orientation::Horizontal,
        constants::PIN_BADGE_LABEL_GAP,
    );
    hbox.set_halign(gtk4::Align::Center);

    let badge = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    badge.add_css_class("pin-badge");
    badge.set_size_request(constants::PIN_BADGE_SIZE, constants::PIN_BADGE_SIZE);
    badge.set_valign(gtk4::Align::Center);

    hbox.append(&badge);
    hbox.append(&label_widget);

    vbox.append(&hbox);
}

/// Strips desktop field codes (%u, %F, %%, etc.) from an Exec command.
/// Per the freedesktop Desktop Entry spec, recognised single-letter codes are
/// removed and `%%` is collapsed to a literal `%`. Arguments after field codes
/// are preserved. Quotes are preserved — shell splitting happens at launch time
/// via shell_words::split() (issue #11).
pub fn strip_field_codes(exec: &str) -> String {
    let mut result = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.peek() {
                // %% → literal %
                Some('%') => {
                    chars.next();
                    result.push('%');
                }
                // Known field codes per freedesktop spec — drop them
                Some('f' | 'F' | 'u' | 'U' | 'd' | 'D' | 'n' | 'N' | 'i' | 'c' | 'k' | 'v' | 'm') => {
                    chars.next();
                    // Trim a single leading space before the field code if present
                    if result.ends_with(' ') && chars.peek().is_none_or(|&ch| ch == ' ') {
                        result.pop();
                    }
                }
                // Unknown %-sequence — keep as-is
                _ => result.push('%'),
            }
        } else {
            result.push(c);
        }
    }
    result.trim().to_string()
}

/// Prepends GTK_THEME= to a command if force-theme is enabled.
pub fn prepend_theme(cmd: &str, theme_prefix: &str) -> String {
    if theme_prefix.is_empty() {
        cmd.to_string()
    } else {
        format!("{} {}", theme_prefix, cmd)
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
    fn strip_field_codes_basic() {
        assert_eq!(strip_field_codes("firefox %u"), "firefox");
        assert_eq!(strip_field_codes("code %F"), "code");
        assert_eq!(strip_field_codes("gimp"), "gimp");
    }

    #[test]
    fn strip_field_codes_preserves_quotes() {
        assert_eq!(strip_field_codes(r#""firefox" %u"#), r#""firefox""#);
        assert_eq!(
            strip_field_codes(r#"sh -c "echo hello" %u"#),
            r#"sh -c "echo hello""#
        );
    }

    #[test]
    fn strip_field_codes_no_space_before_percent() {
        assert_eq!(strip_field_codes("firefox%u"), "firefox");
    }

    #[test]
    fn strip_field_codes_preserves_args_after_code() {
        assert_eq!(
            strip_field_codes("foo %U --new-window"),
            "foo --new-window"
        );
        assert_eq!(strip_field_codes("bar %f --flag %F --other"), "bar --flag --other");
    }

    #[test]
    fn strip_field_codes_literal_percent() {
        assert_eq!(
            strip_field_codes(r#"sh -c "printf '100%%'""#),
            r#"sh -c "printf '100%'""#
        );
    }

    #[test]
    fn strip_field_codes_preserves_inner_whitespace() {
        assert_eq!(
            strip_field_codes(r#"sh -c "printf 'a  b'" %u"#),
            r#"sh -c "printf 'a  b'""#
        );
    }

    #[test]
    fn strip_field_codes_trims_whitespace() {
        assert_eq!(strip_field_codes("  firefox  "), "firefox");
        assert_eq!(strip_field_codes(""), "");
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
