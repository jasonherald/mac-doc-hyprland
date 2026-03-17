use crate::config::DockConfig;
use crate::state::DockState;
use dock_common::compositor::Compositor;
use dock_common::signals::WindowCommand;
use gtk4::glib;
use gtk4::prelude::*;
use notify::{RecursiveMode, Watcher};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;

/// Sets up an inotify-based pin file watcher that triggers a rebuild
/// when the pin file is modified (e.g. by the drawer).
pub fn setup_pin_watcher(pinned_file: &Path, rebuild: &Rc<dyn Fn()>) {
    let pin_path = pinned_file.to_path_buf();
    let rebuild = Rc::clone(rebuild);
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let tx = tx;
        let mut watcher = match notify::recommended_watcher(move |res: Result<notify::Event, _>| {
            if let Ok(event) = res
                && matches!(
                    event.kind,
                    notify::EventKind::Modify(_) | notify::EventKind::Create(_)
                )
            {
                let _ = tx.send(());
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("Pin watcher failed: {}", e);
                return;
            }
        };

        if let Some(parent) = pin_path.parent() {
            let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
        }
        // Block forever — watcher stops if thread exits
        std::thread::park();
    });

    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        if rx.try_recv().is_ok() {
            while rx.try_recv().is_ok() {} // drain
            log::debug!("Pin file changed, rebuilding dock");
            rebuild();
        }
        glib::ControlFlow::Continue
    });
}

/// Sets up a signal handler poller that controls window visibility
/// based on SIGRTMIN+1/2/3 signals.
pub fn setup_signal_poller(
    all_windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    sig_rx: &Rc<mpsc::Receiver<WindowCommand>>,
) {
    let windows = Rc::clone(all_windows);
    let rx = Rc::clone(sig_rx);

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        while let Ok(cmd) = rx.try_recv() {
            for win in windows.borrow().iter() {
                match cmd {
                    WindowCommand::Show => win.set_visible(true),
                    WindowCommand::Hide => win.set_visible(false),
                    WindowCommand::Toggle => win.set_visible(!win.is_visible()),
                    WindowCommand::Quit => win.close(),
                }
            }
        }
        glib::ControlFlow::Continue
    });
}

/// Sets up autohide: hides dock windows after initial show,
/// then starts the compositor IPC cursor edge poller.
pub fn setup_autohide(
    all_windows: &Rc<RefCell<Vec<gtk4::ApplicationWindow>>>,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    compositor: &Rc<dyn Compositor>,
) {
    for win in all_windows.borrow().iter() {
        let win_hide = win.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
            win_hide.set_visible(false);
        });
    }

    crate::ui::hotspot::start_cursor_poller(all_windows, config, state, compositor);
}
