use crate::config::PopupPosition;
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

// Backdrop helpers live in `nwg_common::layer_shell`; the panel and
// DND menu re-export-by-using them with their own CSS class so the
// stylesheet for each gets the right opacity.
