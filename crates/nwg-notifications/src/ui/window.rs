use crate::config::PopupPosition;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;

/// Configures a popup window with layer-shell properties.
pub fn setup_popup_window(win: &gtk4::ApplicationWindow, position: PopupPosition, top_offset: i32) {
    win.init_layer_shell();
    win.set_namespace(Some("nwg-notification-popup"));
    win.set_layer(gtk4_layer_shell::Layer::Overlay);
    win.set_exclusive_zone(-1);

    // Anchor to the correct corner
    match position {
        PopupPosition::TopRight => {
            win.set_anchor(gtk4_layer_shell::Edge::Top, true);
            win.set_anchor(gtk4_layer_shell::Edge::Right, true);
        }
        PopupPosition::TopLeft => {
            win.set_anchor(gtk4_layer_shell::Edge::Top, true);
            win.set_anchor(gtk4_layer_shell::Edge::Left, true);
        }
        PopupPosition::BottomRight => {
            win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
            win.set_anchor(gtk4_layer_shell::Edge::Right, true);
        }
        PopupPosition::BottomLeft => {
            win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
            win.set_anchor(gtk4_layer_shell::Edge::Left, true);
        }
    }

    // Margins
    let is_top = matches!(position, PopupPosition::TopRight | PopupPosition::TopLeft);
    let is_right = matches!(
        position,
        PopupPosition::TopRight | PopupPosition::BottomRight
    );

    if is_top {
        win.set_margin(gtk4_layer_shell::Edge::Top, top_offset);
    } else {
        win.set_margin(gtk4_layer_shell::Edge::Bottom, top_offset);
    }

    if is_right {
        win.set_margin(
            gtk4_layer_shell::Edge::Right,
            super::constants::POPUP_SIDE_MARGIN,
        );
    } else {
        win.set_margin(
            gtk4_layer_shell::Edge::Left,
            super::constants::POPUP_SIDE_MARGIN,
        );
    }

    // No keyboard interactivity — popups shouldn't steal focus
    win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
}

/// Configures a full-screen transparent backdrop for click-outside-to-close.
///
/// Shared by panel and DND menu — anchors all 4 edges, overlay layer, no keyboard.
/// Pins the surface to a specific monitor so a single layer-shell surface
/// only covers that one output. Callers that want coverage across all
/// monitors should use [`create_fullscreen_backdrops`] instead.
pub fn setup_fullscreen_backdrop(
    win: &gtk4::ApplicationWindow,
    namespace: &str,
    monitor: &gtk4::gdk::Monitor,
) {
    win.init_layer_shell();
    win.set_namespace(Some(namespace));
    win.set_layer(gtk4_layer_shell::Layer::Overlay);
    win.set_exclusive_zone(-1);
    win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    win.set_monitor(Some(monitor));
    win.set_anchor(gtk4_layer_shell::Edge::Top, true);
    win.set_anchor(gtk4_layer_shell::Edge::Right, true);
    win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
    win.set_anchor(gtk4_layer_shell::Edge::Left, true);
}

/// Creates one full-screen transparent backdrop window per connected monitor.
///
/// A layer-shell surface only covers the output it's pinned to, so a single
/// backdrop can't catch clicks on other monitors (issue #55). Callers create
/// a Vec of these and toggle them together as a single logical backdrop.
/// If GDK reports no monitors — rare, usually a headless/early-startup
/// transient — returns an empty Vec and the caller falls back to whatever
/// degraded behavior makes sense (the panel/menu will still toggle, just
/// without click-outside-to-close).
pub fn create_fullscreen_backdrops(
    app: &gtk4::Application,
    namespace: &str,
) -> Vec<gtk4::ApplicationWindow> {
    let Some(display) = gtk4::gdk::Display::default() else {
        log::warn!("No default GDK display — backdrops disabled");
        return Vec::new();
    };
    let monitors_model = display.monitors();
    let mut backdrops = Vec::with_capacity(monitors_model.n_items() as usize);
    for i in 0..monitors_model.n_items() {
        let Some(item) = monitors_model.item(i) else {
            continue;
        };
        let Ok(monitor) = item.downcast::<gtk4::gdk::Monitor>() else {
            continue;
        };
        let win = gtk4::ApplicationWindow::new(app);
        win.add_css_class("notification-backdrop");
        setup_fullscreen_backdrop(&win, namespace, &monitor);
        backdrops.push(win);
    }
    backdrops
}
