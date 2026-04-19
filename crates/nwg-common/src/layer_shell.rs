//! Shared layer-shell helpers used by drawer, notifications panel, and DND menu.
//!
//! A layer-shell surface only covers the output it's pinned to, so any UI
//! element that wants "click anywhere on any monitor closes me" needs one
//! catcher surface per monitor (issue #55). This module centralizes that
//! pattern so multiple crates don't drift in their backdrop construction.

use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;

/// Configures a single full-screen transparent layer-shell window pinned to
/// `monitor`. Anchors all four edges, overlay layer, no exclusive zone, no
/// keyboard input. Caller is responsible for adding any CSS class that
/// gives the window a non-zero background opacity — without that, some
/// compositors won't deliver pointer events to it.
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

/// Creates one full-screen transparent backdrop window per connected monitor,
/// each tagged with `css_class` so the caller's stylesheet can give it the
/// minimum opacity needed for pointer-event delivery.
///
/// `exclude_connector` filters out a single monitor by its GDK connector
/// name (e.g. `"DP-1"`). Callers that already render their own full-screen
/// surface on a particular monitor should pass that monitor's connector
/// here — otherwise the backdrop on the same monitor would race with the
/// caller's surface for click delivery (drawer use-case, issue #55).
/// Pass `None` to cover every monitor.
///
/// Returns an empty Vec if GDK reports no display (rare; usually a headless
/// or early-startup transient). Callers should treat the result as the full
/// backdrop set and toggle them all together.
pub fn create_fullscreen_backdrops(
    app: &gtk4::Application,
    namespace: &str,
    css_class: &str,
    exclude_connector: Option<&str>,
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
        if let Some(skip) = exclude_connector
            && monitor.connector().is_some_and(|c| c == skip)
        {
            continue;
        }
        let win = gtk4::ApplicationWindow::new(app);
        win.add_css_class(css_class);
        setup_fullscreen_backdrop(&win, namespace, &monitor);
        backdrops.push(win);
    }
    backdrops
}
