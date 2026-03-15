use super::constants::*;
use crate::notification::Notification;
use dock_common::desktop::icons;
use gtk4::prelude::*;
use std::path::PathBuf;
use std::time::SystemTime;

/// Builds a single notification row for the history panel.
pub fn build_row(
    notif: &Notification,
    app_dirs: &[PathBuf],
    on_click: impl Fn(u32) + 'static,
    on_dismiss: impl Fn(u32) + 'static,
) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    row.add_css_class("notification-row");
    row.set_margin_start(4);
    row.set_margin_end(4);
    row.set_margin_top(2);
    row.set_margin_bottom(2);

    // App icon (small for panel)
    let icon = resolve_icon(&notif.app_icon, &notif.app_name, app_dirs);
    icon.set_pixel_size(PANEL_ICON_SIZE);
    icon.set_valign(gtk4::Align::Start);
    icon.set_margin_top(4);
    row.append(&icon);

    // Text column
    let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
    text_box.set_hexpand(true);

    // Summary + time on same line
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let summary = gtk4::Label::new(Some(&notif.summary));
    summary.add_css_class("row-summary");
    summary.set_halign(gtk4::Align::Start);
    summary.set_hexpand(true);
    summary.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    summary.set_max_width_chars(35);
    header.append(&summary);

    let time_str = relative_time(notif.timestamp);
    let time_label = gtk4::Label::new(Some(&time_str));
    time_label.add_css_class("row-time");
    header.append(&time_label);
    text_box.append(&header);

    // Body
    if !notif.body.is_empty() {
        let body = gtk4::Label::new(Some(&notif.body));
        body.add_css_class("row-body");
        body.set_halign(gtk4::Align::Start);
        body.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        body.set_max_width_chars(45);
        body.set_lines(2);
        body.set_wrap(true);
        text_box.append(&body);
    }

    row.append(&text_box);

    // Dismiss button
    let dismiss_btn = gtk4::Button::from_icon_name("window-close-symbolic");
    dismiss_btn.add_css_class("row-dismiss");
    dismiss_btn.set_valign(gtk4::Align::Start);
    dismiss_btn.set_margin_top(4);
    let dismiss_id = notif.id;
    dismiss_btn.connect_clicked(move |_| on_dismiss(dismiss_id));
    row.append(&dismiss_btn);

    // Click row → focus app
    let click_id = notif.id;
    let gesture = gtk4::GestureClick::new();
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        on_click(click_id);
    });
    row.add_controller(gesture);

    row
}

/// Resolves icon using GTK4 icon theme (avoids glycin crashes).
fn resolve_icon(app_icon: &str, app_name: &str, app_dirs: &[PathBuf]) -> gtk4::Image {
    // Try app_icon as icon theme name
    if !app_icon.is_empty() && !app_icon.contains('/') {
        let img = gtk4::Image::from_icon_name(app_icon);
        img.set_pixel_size(PANEL_ICON_SIZE);
        return img;
    }
    // Try desktop entry lookup
    if let Some(icon_name) = icons::get_icon(app_name, app_dirs)
        && !icon_name.contains('/')
    {
        let img = gtk4::Image::from_icon_name(&icon_name);
        img.set_pixel_size(PANEL_ICON_SIZE);
        return img;
    }
    // Fallback
    let img = gtk4::Image::from_icon_name("dialog-information");
    img.set_pixel_size(PANEL_ICON_SIZE);
    img
}

fn relative_time(timestamp: SystemTime) -> String {
    let elapsed = timestamp.elapsed().unwrap_or_default();
    let secs = elapsed.as_secs();
    if secs < 60 {
        "now".into()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}
