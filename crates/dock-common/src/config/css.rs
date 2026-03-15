use gtk4::gdk;
use std::path::Path;

/// Loads a CSS file and applies it application-wide.
/// Returns true if the CSS was loaded successfully.
///
/// Note: panics if no GTK display is available (app can't function without one).
pub fn load_css(css_path: &Path) -> bool {
    let provider = gtk4::CssProvider::new();

    if css_path.exists() {
        provider.load_from_path(css_path);
        log::info!("Loaded CSS from {}", css_path.display());
    } else {
        log::warn!("{} not found, using default GTK styling", css_path.display());
        return false;
    }

    apply_provider(&provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    true
}

/// Loads CSS from a string and applies it application-wide.
///
/// Applied at higher priority than file-based CSS so overrides take effect.
pub fn load_css_from_data(css: &str) -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(css);
    apply_provider(&provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
    provider
}

fn apply_provider(provider: &gtk4::CssProvider, priority: u32) {
    let display = gdk::Display::default()
        .expect("GTK display not available — is GTK initialized?");
    gtk4::style_context_add_provider_for_display(&display, provider, priority);
}
