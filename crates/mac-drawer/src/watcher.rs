use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;

/// Events from the file watcher.
#[derive(Debug)]
pub enum WatchEvent {
    /// Desktop files changed (app added/removed).
    DesktopFilesChanged,
    /// Pinned file changed.
    PinnedChanged,
}

/// Starts watching app directories and the pin cache file for changes.
/// Returns a receiver that emits WatchEvent when relevant files change.
pub fn start_watcher(
    app_dirs: &[std::path::PathBuf],
    pin_file: &Path,
) -> mpsc::Receiver<WatchEvent> {
    let (tx, rx) = mpsc::channel();

    let pin_path = pin_file.to_path_buf();
    let app_dir_list: Vec<_> = app_dirs.to_vec();

    std::thread::spawn(move || {
        let (notify_tx, notify_rx) = mpsc::channel();

        let mut watcher = match notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                let _ = notify_tx.send(event);
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create file watcher: {}", e);
                return;
            }
        };

        // Watch app directories
        for dir in &app_dir_list {
            if dir.exists()
                && let Err(e) = watcher.watch(dir, RecursiveMode::Recursive)
            {
                log::warn!("Failed to watch {}: {}", dir.display(), e);
            }
        }

        // Watch pin file's parent directory (to catch creation)
        if let Some(parent) = pin_path.parent()
            && parent.exists()
            && let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive)
        {
            log::warn!("Failed to watch {}: {}", parent.display(), e);
        }

        for event in notify_rx {
            match event.kind {
                EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
                    let is_pin = event.paths.iter().any(|p| p == &pin_path);
                    let is_desktop = event
                        .paths
                        .iter()
                        .any(|p| p.extension().is_some_and(|ext| ext == "desktop"));

                    if is_pin {
                        let _ = tx.send(WatchEvent::PinnedChanged);
                    } else if is_desktop {
                        let _ = tx.send(WatchEvent::DesktopFilesChanged);
                    }
                }
                _ => {}
            }
        }
    });

    rx
}
