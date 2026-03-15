use gtk4::gdk;
use std::path::Path;

/// Loads a CSS file and applies it application-wide.
/// Returns true if the CSS was loaded successfully.
pub fn load_css(css_path: &Path) -> bool {
    let provider = gtk4::CssProvider::new();

    if css_path.exists() {
        provider.load_from_path(css_path);
        log::info!("Using style: {}", css_path.display());
    } else {
        log::warn!("{} not found, using GTK styling", css_path.display());
        return false;
    }

    let display = gdk::Display::default().expect("Could not get default display");
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    true
}

/// Loads CSS from a data string and applies it application-wide.
pub fn load_css_from_data(css: &str) -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(css);

    let display = gdk::Display::default().expect("Could not get default display");
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    provider
}
