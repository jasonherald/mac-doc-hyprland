use gtk4::gdk;
use std::path::Path;

/// CSS priority: embedded defaults (base layer).
const CSS_PRIORITY_EMBEDDED: u32 = gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION;
/// CSS priority: programmatic overrides like --opacity (middle layer).
const CSS_PRIORITY_OVERRIDE: u32 = CSS_PRIORITY_EMBEDDED + 1;
/// CSS priority: user CSS file (highest — always wins, including after hot-reload).
const CSS_PRIORITY_USER: u32 = CSS_PRIORITY_EMBEDDED + 2;

/// Debounce interval for CSS file change detection (milliseconds).
const CSS_RELOAD_DEBOUNCE_MS: u64 = 100;

/// Loads a CSS file and applies it at the highest priority (user overrides).
/// Returns the CssProvider (for hot-reload) or None if the file doesn't exist.
///
/// Priority order: embedded defaults < programmatic overrides < user CSS file.
/// This ensures user CSS always wins, including after hot-reload.
pub fn load_css(css_path: &Path) -> Option<gtk4::CssProvider> {
    let provider = gtk4::CssProvider::new();

    if css_path.exists() {
        provider.load_from_path(css_path);
        log::info!("Loaded CSS from {}", css_path.display());
    } else {
        log::warn!(
            "{} not found, using default GTK styling",
            css_path.display()
        );
        return None;
    }

    apply_provider(&provider, CSS_PRIORITY_USER);
    Some(provider)
}

/// Loads CSS from a string as embedded defaults (lowest priority).
///
/// User CSS file and programmatic overrides both take precedence.
pub fn load_css_from_data(css: &str) -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(css);
    apply_provider(&provider, CSS_PRIORITY_EMBEDDED);
    provider
}

/// Loads CSS from a string as a programmatic override (middle priority).
///
/// Overrides embedded defaults, but user CSS file still wins.
pub fn load_css_override(css: &str) -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(css);
    apply_provider(&provider, CSS_PRIORITY_OVERRIDE);
    provider
}

/// Watches a CSS file for changes and reloads the provider automatically.
/// Uses inotify (Linux) via the `notify` crate — no polling.
/// The watcher thread runs until the provider is dropped.
pub fn watch_css(css_path: &Path, provider: &gtk4::CssProvider) {
    let path = css_path.to_path_buf();
    let provider = provider.clone();

    // Watch the parent directory (inotify can't watch a file that might be
    // replaced atomically via rename, which is how most editors save).
    let watch_dir = match path.parent() {
        Some(dir) => dir.to_path_buf(),
        None => {
            log::debug!("CSS watch skipped: no parent directory for {}", path.display());
            return;
        }
    };
    let file_name = match path.file_name() {
        Some(name) => name.to_os_string(),
        None => {
            log::debug!("CSS watch skipped: no filename for {}", path.display());
            return;
        }
    };

    let (tx, rx) = std::sync::mpsc::channel::<()>();

    std::thread::spawn(move || {
        use notify::{RecursiveMode, Watcher};
        let mut watcher = match notify::recommended_watcher(make_css_handler(file_name, tx)) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("Failed to create CSS watcher: {}", e);
                return;
            }
        };
        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
            log::warn!(
                "Failed to watch CSS directory '{}': {}",
                watch_dir.display(),
                e
            );
            return;
        }
        // Block forever — watcher stops if thread exits
        loop {
            std::thread::park();
        }
    });

    // Debounced reload on the GTK main thread
    let path_reload = path.clone();
    gtk4::glib::timeout_add_local(
        std::time::Duration::from_millis(CSS_RELOAD_DEBOUNCE_MS),
        move || {
            let mut changed = false;
            while rx.try_recv().is_ok() {
                changed = true;
            }
            if changed {
                log::info!("CSS file changed, reloading: {}", path_reload.display());
                if path_reload.exists() {
                    provider.load_from_path(&path_reload);
                } else {
                    // File deleted — clear user styles so defaults show through
                    provider.load_from_data("");
                }
            }
            gtk4::glib::ControlFlow::Continue
        },
    );
}

/// Creates a notify event handler that sends on the channel when the
/// target CSS file is modified (by any save strategy, including deletion).
fn make_css_handler(
    file_name: std::ffi::OsString,
    tx: std::sync::mpsc::Sender<()>,
) -> impl FnMut(Result<notify::Event, notify::Error>) {
    move |event| {
        let ev = match event {
            Ok(ev) => ev,
            Err(e) => {
                log::warn!("CSS watcher error: {}", e);
                return;
            }
        };
        let matches_file = ev
            .paths
            .iter()
            .any(|p| p.file_name().is_some_and(|name| name == file_name));
        if matches_file {
            if let Err(e) = tx.send(()) {
                log::warn!("CSS watcher channel closed: {}", e);
            }
        }
    }
}

fn apply_provider(provider: &gtk4::CssProvider, priority: u32) {
    let Some(display) = gdk::Display::default() else {
        log::error!("Cannot apply CSS: GTK display not available — is GTK initialized?");
        return;
    };
    gtk4::style_context_add_provider_for_display(&display, provider, priority);
}
