use dock_common::config::css;
use std::path::Path;

/// Loads the dock's CSS file. Returns true if loaded successfully.
pub fn load_dock_css(css_path: &Path) -> bool {
    css::load_css(css_path)
}

/// Loads the hotspot CSS. Uses internal default if file not found.
pub fn load_hotspot_css(css_path: &Path) -> gtk4::CssProvider {
    if css_path.exists() {
        let provider = gtk4::CssProvider::new();
        provider.load_from_path(css_path);
        log::info!("Hotspot css loaded from {}", css_path.display());

        let display = gtk4::gdk::Display::default().expect("No display");
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        provider
    } else {
        log::info!(
            "Optional '{}' file not found, using internal definition",
            css_path.display()
        );
        css::load_css_from_data("window { all: unset; }")
    }
}
