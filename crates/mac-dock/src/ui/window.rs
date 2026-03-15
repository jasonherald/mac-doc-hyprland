use crate::config::DockConfig;
use gtk4_layer_shell::LayerShell;

/// Configures the main dock window with layer-shell properties.
pub fn setup_dock_window(win: &gtk4::ApplicationWindow, config: &DockConfig) {
    win.init_layer_shell();
    win.set_namespace(Some("mac-dock"));

    // Position anchoring
    match config.position.as_str() {
        "bottom" => {
            win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
            win.set_anchor(gtk4_layer_shell::Edge::Left, config.full);
            win.set_anchor(gtk4_layer_shell::Edge::Right, config.full);
        }
        "top" => {
            win.set_anchor(gtk4_layer_shell::Edge::Top, true);
            win.set_anchor(gtk4_layer_shell::Edge::Left, config.full);
            win.set_anchor(gtk4_layer_shell::Edge::Right, config.full);
        }
        "left" => {
            win.set_anchor(gtk4_layer_shell::Edge::Left, true);
            win.set_anchor(gtk4_layer_shell::Edge::Top, config.full);
            win.set_anchor(gtk4_layer_shell::Edge::Bottom, config.full);
        }
        "right" => {
            win.set_anchor(gtk4_layer_shell::Edge::Right, true);
            win.set_anchor(gtk4_layer_shell::Edge::Top, config.full);
            win.set_anchor(gtk4_layer_shell::Edge::Bottom, config.full);
        }
        _ => {
            win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
        }
    }

    // Layer and exclusive zone
    let mut layer_str = config.layer.clone();
    if config.exclusive {
        win.auto_exclusive_zone_enable();
        layer_str = "top".to_string();
    }

    match layer_str.as_str() {
        "top" => win.set_layer(gtk4_layer_shell::Layer::Top),
        "bottom" => win.set_layer(gtk4_layer_shell::Layer::Bottom),
        _ => {
            win.set_layer(gtk4_layer_shell::Layer::Overlay);
            win.set_exclusive_zone(-1);
        }
    }

    // Margins
    win.set_margin(gtk4_layer_shell::Edge::Top, config.mt);
    win.set_margin(gtk4_layer_shell::Edge::Left, config.ml);
    win.set_margin(gtk4_layer_shell::Edge::Right, config.mr);
    win.set_margin(gtk4_layer_shell::Edge::Bottom, config.mb);
}

/// Returns the (outer_orientation, inner_orientation) for the dock position.
pub fn orientations(config: &DockConfig) -> (gtk4::Orientation, gtk4::Orientation) {
    if config.is_vertical() {
        (gtk4::Orientation::Horizontal, gtk4::Orientation::Vertical)
    } else {
        (gtk4::Orientation::Vertical, gtk4::Orientation::Horizontal)
    }
}
