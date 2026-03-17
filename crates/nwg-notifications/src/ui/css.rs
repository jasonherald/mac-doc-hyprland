use nwg_dock_common::config::css;

/// Embedded notification CSS, loaded at compile time.
const NOTIFICATION_CSS: &str = include_str!("../assets/notifications.css");

/// Loads the notification CSS styling.
pub fn load_notification_css() {
    css::load_css_from_data(NOTIFICATION_CSS);
}
