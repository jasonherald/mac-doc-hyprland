use dock_common::config::css;
use std::path::Path;

/// GTK4 overrides for mac-style dock rendering.
/// Compact buttons, transparent background, tight indicator spacing.
const GTK4_COMPAT_CSS: &str = r#"
window {
    background-color: rgba(54, 54, 79, 0.75);
}
.dock-button {
    min-height: 0;
    min-width: 0;
}
.dock-button image {
    margin: 0;
    padding: 0;
}
.dock-indicator {
    margin: 0;
    padding: 0;
    min-height: 0;
    min-width: 0;
}
"#;

/// Loads the dock's CSS file and applies GTK4 compatibility overrides.
/// Returns true if the user's CSS was loaded successfully.
pub fn load_dock_css(css_path: &Path) -> bool {
    let result = css::load_css(css_path);
    // Apply GTK4 button overrides at higher priority so they take effect
    // after the user's style.css
    css::load_css_from_data(GTK4_COMPAT_CSS);
    result
}

/// Loads the hotspot CSS. Uses internal default if file not found.
#[allow(dead_code)]
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
