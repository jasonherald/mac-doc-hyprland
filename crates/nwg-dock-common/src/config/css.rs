use gtk4::gdk;
use std::path::Path;
use std::sync::mpsc::TryRecvError;

/// CSS priority: embedded defaults (base layer).
const CSS_PRIORITY_EMBEDDED: u32 = gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION;
/// CSS priority: programmatic overrides like --opacity (middle layer).
const CSS_PRIORITY_OVERRIDE: u32 = CSS_PRIORITY_EMBEDDED + 1;
/// CSS priority: user CSS file (highest — always wins, including after hot-reload).
const CSS_PRIORITY_USER: u32 = CSS_PRIORITY_EMBEDDED + 2;

/// Debounce interval for CSS file change detection (milliseconds).
const CSS_RELOAD_DEBOUNCE_MS: u64 = 100;

/// Loads a CSS file and applies it at the highest priority (user overrides).
/// Always returns a CssProvider — if the file doesn't exist yet, an empty
/// provider is installed so `watch_css` can hot-load it when created.
///
/// Priority order: embedded defaults < programmatic overrides < user CSS file.
/// This ensures user CSS always wins, including after hot-reload.
pub fn load_css(css_path: &Path) -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();

    if css_path.exists() {
        provider.load_from_path(css_path);
        log::info!("Loaded CSS from {}", css_path.display());
    } else {
        log::info!("{} not found — watching for creation", css_path.display());
    }

    apply_provider(&provider, CSS_PRIORITY_USER);
    provider
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
    let Some((watch_dir, file_name)) = split_watch_target(&path) else {
        return;
    };

    let (tx, rx) = std::sync::mpsc::channel::<()>();
    spawn_watcher_thread(watch_dir, file_name, tx);
    install_reload_timer(path, provider.clone(), rx);
}

/// Splits a CSS path into (parent_dir, filename). Logs and returns None
/// if either component is missing.
fn split_watch_target(path: &Path) -> Option<(std::path::PathBuf, std::ffi::OsString)> {
    let watch_dir = path.parent().map(|d| d.to_path_buf()).or_else(|| {
        log::debug!(
            "CSS watch skipped: no parent directory for {}",
            path.display()
        );
        None
    })?;
    let file_name = path.file_name().map(|n| n.to_os_string()).or_else(|| {
        log::debug!("CSS watch skipped: no filename for {}", path.display());
        None
    })?;
    Some((watch_dir, file_name))
}

/// Spawns the notify watcher on a background thread. The thread runs until
/// the process exits — there's no cleanup path, which is fine for a daemon.
fn spawn_watcher_thread(
    watch_dir: std::path::PathBuf,
    file_name: std::ffi::OsString,
    tx: std::sync::mpsc::Sender<()>,
) {
    std::thread::spawn(move || {
        use notify::{RecursiveMode, Watcher};
        let Ok(mut watcher) = notify::recommended_watcher(make_css_handler(file_name, tx))
            .inspect_err(|e| log::warn!("Failed to create CSS watcher: {}", e))
        else {
            return;
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
}

/// Installs a debounced GLib timer that reloads the provider when
/// the watcher signals a file change. Stops the source on disconnect.
fn install_reload_timer(
    path: std::path::PathBuf,
    provider: gtk4::CssProvider,
    rx: std::sync::mpsc::Receiver<()>,
) {
    gtk4::glib::timeout_add_local(
        std::time::Duration::from_millis(CSS_RELOAD_DEBOUNCE_MS),
        move || match drain_events(&rx) {
            DrainResult::Changed => {
                reload_provider(&provider, &path);
                gtk4::glib::ControlFlow::Continue
            }
            DrainResult::Empty => gtk4::glib::ControlFlow::Continue,
            DrainResult::Disconnected => {
                log::warn!("CSS watcher disconnected; stopping hot-reload");
                gtk4::glib::ControlFlow::Break
            }
        },
    );
}

enum DrainResult {
    Changed,
    Empty,
    Disconnected,
}

/// Drains all pending events from the watcher channel.
fn drain_events(rx: &std::sync::mpsc::Receiver<()>) -> DrainResult {
    let mut changed = false;
    loop {
        match rx.try_recv() {
            Ok(()) => changed = true,
            Err(TryRecvError::Empty) => {
                return if changed {
                    DrainResult::Changed
                } else {
                    DrainResult::Empty
                };
            }
            Err(TryRecvError::Disconnected) => return DrainResult::Disconnected,
        }
    }
}

/// Reloads the CSS provider from the file, or clears it if the file is gone.
fn reload_provider(provider: &gtk4::CssProvider, path: &Path) {
    log::info!("CSS file changed, reloading: {}", path.display());
    if path.exists() {
        provider.load_from_path(path);
    } else {
        // File deleted — clear user styles so defaults show through
        provider.load_from_data("");
    }
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
        if matches_file && let Err(e) = tx.send(()) {
            log::warn!("CSS watcher channel closed: {}", e);
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
