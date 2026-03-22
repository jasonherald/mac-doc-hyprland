use crate::config::DockConfig;
use crate::dock_windows::{self, MonitorDock};
use crate::monitor;
use crate::state::DockState;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use notify::{RecursiveMode, Watcher};
use nwg_dock_common::compositor::Compositor;
use nwg_dock_common::signals::WindowCommand;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

/// Delay before hiding dock windows after initial present (allows GTK to render).
const AUTOHIDE_INITIAL_DELAY: Duration = Duration::from_millis(500);

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
    app: &gtk4::Application,
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    sig_rx: &Rc<mpsc::Receiver<WindowCommand>>,
) {
    let app = app.clone();
    let docks = Rc::clone(per_monitor);
    let rx = Rc::clone(sig_rx);

    glib::timeout_add_local(Duration::from_millis(100), move || {
        while let Ok(cmd) = rx.try_recv() {
            // Quit shuts down the entire application (including hotspot windows)
            if matches!(cmd, WindowCommand::Quit) {
                app.quit();
                return glib::ControlFlow::Break;
            }
            let toggle_to = !docks.borrow().iter().any(|d| d.win.is_visible());
            for dock in docks.borrow().iter() {
                match cmd {
                    WindowCommand::Show => dock.win.set_visible(true),
                    WindowCommand::Hide => dock.win.set_visible(false),
                    WindowCommand::Toggle => dock.win.set_visible(toggle_to),
                    WindowCommand::Quit => unreachable!(),
                }
            }
        }
        glib::ControlFlow::Continue
    });
}

/// Sets up autohide: hides dock windows after initial show,
/// then starts the appropriate autohide mechanism for the compositor.
/// Returns a `HotspotContext` for Sway (used by reconciliation to create
/// hotspot windows for hotplugged monitors).
pub fn setup_autohide(
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    compositor: &Rc<dyn Compositor>,
    app: &gtk4::Application,
) -> Option<Rc<crate::ui::hotspot::HotspotContext>> {
    for dock in per_monitor.borrow().iter() {
        let win = dock.win.clone();
        glib::timeout_add_local_once(AUTOHIDE_INITIAL_DELAY, move || {
            win.set_visible(false);
        });
    }

    crate::ui::hotspot::setup_autohide(per_monitor, config, state, compositor, app)
}

/// Watches for GDK display monitor changes and reconciles dock windows.
///
/// Uses the `items-changed` signal on `Display::monitors()` to detect
/// monitor hotplug events. Debounced via idle callback to coalesce
/// rapid events (e.g., unplug + replug).
pub fn setup_monitor_watcher(
    app: &gtk4::Application,
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &Rc<DockConfig>,
    rebuild_fn: &Rc<dyn Fn()>,
    hotspot_ctx: Option<Rc<crate::ui::hotspot::HotspotContext>>,
) {
    let Some(display) = gtk4::gdk::Display::default() else {
        log::error!("No default GDK display for monitor watcher");
        return;
    };

    let model = display.monitors();
    let pending = Rc::new(Cell::new(false));
    let app = app.clone();
    let per_monitor = Rc::clone(per_monitor);
    let config = Rc::clone(config);
    let rebuild_fn = Rc::clone(rebuild_fn);

    model.connect_items_changed(move |_, _, _, _| {
        if pending.get() {
            return; // already scheduled
        }
        pending.set(true);

        let pending = Rc::clone(&pending);
        let app = app.clone();
        let per_monitor = Rc::clone(&per_monitor);
        let config = Rc::clone(&config);
        let rebuild_fn = Rc::clone(&rebuild_fn);
        let hotspot_ctx = hotspot_ctx.clone();

        glib::idle_add_local_once(move || {
            pending.set(false);
            log::info!("Monitor topology changed, reconciling dock windows");
            reconcile_monitors(&app, &per_monitor, &config, &rebuild_fn, hotspot_ctx.as_deref());
        });
    });
}

/// Reconciles dock windows with current monitor topology.
/// Creates windows for new monitors, destroys windows for removed monitors.
fn reconcile_monitors(
    app: &gtk4::Application,
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &DockConfig,
    rebuild_fn: &Rc<dyn Fn()>,
    hotspot_ctx: Option<&crate::ui::hotspot::HotspotContext>,
) {
    let current_monitors = monitor::resolve_monitors(config);
    let monitor_map: std::collections::HashMap<String, gtk4::gdk::Monitor> =
        current_monitors.into_iter().collect();
    let current_names: Vec<String> = monitor_map.keys().cloned().collect();
    let existing_names: Vec<String> = per_monitor
        .borrow()
        .iter()
        .map(|d| d.output_name.clone())
        .collect();

    let (to_add, to_remove) = dock_windows::compute_monitor_diff(&existing_names, &current_names);

    // Always refresh GDK monitor references — a reconnected monitor with the same
    // connector name produces a new gdk::Monitor object, and the old one is stale.
    for dock in per_monitor.borrow().iter() {
        if let Some(mon) = monitor_map.get(&dock.output_name) {
            dock.win.set_monitor(Some(mon));
        }
    }

    if to_add.is_empty() && to_remove.is_empty() {
        log::debug!("Monitor topology unchanged after debounce");
        return;
    }

    // Remove orphaned dock windows and their hotspot windows
    for name in &to_remove {
        if let Some(ctx) = hotspot_ctx {
            ctx.remove_hotspot_for_output(name);
        }
        per_monitor.borrow_mut().retain(|dock| {
            if &dock.output_name == name {
                log::info!("Removing dock window for disconnected monitor: {}", name);
                dock.win.close();
                false
            } else {
                true
            }
        });
    }

    // Create dock windows for new monitors
    for name in &to_add {
        if let Some(gdk_mon) = monitor_map.get(name) {
            log::info!("Creating dock window for new monitor: {}", name);
            let dock = dock_windows::create_single_dock_window(app, name, gdk_mon, config);
            dock.win.present();
            if config.autohide {
                let win = dock.win.clone();
                glib::timeout_add_local_once(AUTOHIDE_INITIAL_DELAY, move || {
                    win.set_visible(false);
                });
            }
            // Create Sway hotspot window for the new dock if needed
            if let Some(ctx) = hotspot_ctx {
                ctx.add_hotspot_for_dock(&dock);
            }
            per_monitor.borrow_mut().push(dock);
        }
    }

    // Rebuild content in all windows (new windows need buttons)
    rebuild_fn();
}
