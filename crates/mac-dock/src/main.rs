mod config;
mod events;
mod monitor;
mod state;
mod ui;

use crate::config::DockConfig;
use crate::state::DockState;
use clap::Parser;
use dock_common::config::paths;
use dock_common::desktop::dirs::get_app_dirs;
use dock_common::pinning;
use dock_common::signals::{self, WindowCommand};
use dock_common::singleton;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

fn main() {
    let config = DockConfig::parse();

    if config.debug {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::init();
    }

    let mut config = config;
    if config.autohide && config.resident {
        log::warn!("autohide and resident are mutually exclusive, ignoring -d!");
        config.autohide = false;
    }

    if dock_common::hyprland::ipc::instance_signature().is_err() {
        log::error!("HYPRLAND_INSTANCE_SIGNATURE not found, terminating.");
        std::process::exit(1);
    }

    let _lock = if !config.multi {
        match singleton::acquire_lock("mac-dock") {
            Ok(lock) => Some(lock),
            Err(existing_pid) => {
                if let Some(pid) = existing_pid {
                    if config.is_resident_mode() {
                        log::info!("Running instance found (pid {}), terminating...", pid);
                    } else {
                        signals::send_signal_to_pid(pid, signals::sig_toggle());
                        log::info!("Sent toggle signal to running instance (pid {}), bye!", pid);
                    }
                }
                std::process::exit(0);
            }
        }
    } else {
        None
    };

    let data_home = paths::find_data_home("nwg-dock-hyprland").unwrap_or_else(|| {
        log::error!("No data directory found for nwg-dock-hyprland");
        PathBuf::from("/usr/share")
    });
    log::info!("Data home: {}", data_home.display());

    let config_dir = paths::config_dir("nwg-dock-hyprland");
    paths::ensure_dir(&config_dir);

    let css_path = config_dir.join(&config.css_file);
    if !css_path.exists() {
        let src = data_home.join("nwg-dock-hyprland/style.css");
        if let Err(e) = paths::copy_file(&src, &css_path) {
            log::warn!("Error copying default CSS: {}", e);
        }
    }

    let cache_dir = paths::cache_dir().expect("Couldn't determine cache directory");
    let pinned_file = cache_dir.join("mac-dock-pinned");
    let app_dirs = get_app_dirs();
    let sig_rx = Rc::new(signals::setup_signal_handlers(config.is_resident_mode()));

    let app = gtk4::Application::builder()
        .application_id("com.mac-dock.hyprland")
        .build();

    let config = Rc::new(config);
    let data_home = Rc::new(data_home);
    let pinned_file = Rc::new(pinned_file);
    let css_path_rc = Rc::new(css_path);

    app.connect_activate(move |app| {
        let config = Rc::clone(&config);
        let data_home = Rc::clone(&data_home);
        let pinned_file = Rc::clone(&pinned_file);

        // Load CSS
        ui::css::load_dock_css(&css_path_rc);

        // Prevent GTK from quitting when dock windows hide (autohide mode)
        let _hold = app.hold();

        // Create shared state
        let state = Rc::new(RefCell::new(DockState::new(app_dirs.clone())));
        state.borrow_mut().pinned = pinning::load_pinned(&pinned_file);

        if let Err(e) = state.borrow_mut().refresh_clients() {
            log::error!("Couldn't list clients: {}", e);
        }

        // Determine which monitors to show dock on
        let output_map = monitor::map_outputs(&state);
        let monitors: Vec<gtk4::gdk::Monitor> = if !config.output.is_empty() {
            // Single monitor specified
            if let Some(mon) = output_map.get(&config.output) {
                vec![mon.clone()]
            } else {
                log::warn!("Target output '{}' not found, using all monitors", config.output);
                monitor::list_gdk_monitors()
            }
        } else {
            // All monitors
            monitor::list_gdk_monitors()
        };

        log::info!("Creating dock on {} monitor(s)", monitors.len());

        // Track all dock windows for signal handling
        let all_windows: Rc<RefCell<Vec<gtk4::ApplicationWindow>>> =
            Rc::new(RefCell::new(Vec::new()));

        // Shared rebuild holder — all windows share one rebuild function
        type RebuildHolder = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
        let rebuild_holder: RebuildHolder = Rc::new(RefCell::new(None));

        // Per-monitor state for rebuild: (alignment_box, current_main_box, win)
        type PerMonitor = Vec<(gtk4::Box, Rc<RefCell<Option<gtk4::Box>>>, gtk4::ApplicationWindow)>;
        let per_monitor: Rc<RefCell<PerMonitor>> = Rc::new(RefCell::new(Vec::new()));

        // Create a dock window on each monitor
        for mon in &monitors {
            let win = gtk4::ApplicationWindow::new(app);
            ui::window::setup_dock_window(&win, &config);
            win.set_monitor(Some(mon));

            // Outer container
            let (outer_orient, _) = ui::window::orientations(&config);
            let outer_box = gtk4::Box::new(outer_orient, 0);
            outer_box.set_widget_name("box");
            win.set_child(Some(&outer_box));

            let inner_orient = if config.is_vertical() {
                gtk4::Orientation::Vertical
            } else {
                gtk4::Orientation::Horizontal
            };
            let alignment_box = gtk4::Box::new(inner_orient, 0);
            if config.full {
                alignment_box.set_hexpand(true);
                alignment_box.set_vexpand(true);
            }
            outer_box.append(&alignment_box);

            let current_main_box: Rc<RefCell<Option<gtk4::Box>>> =
                Rc::new(RefCell::new(None));

            per_monitor.borrow_mut().push((
                alignment_box.clone(),
                Rc::clone(&current_main_box),
                win.clone(),
            ));

            // Autohide is handled entirely by the Hyprland IPC cursor poller.
            // No GTK EventControllerMotion needed — avoids RefCell borrow conflicts.

            all_windows.borrow_mut().push(win);
        }

        // Build the rebuild function — rebuilds dock content on ALL monitors
        {
            let config = Rc::clone(&config);
            let state = Rc::clone(&state);
            let data_home = Rc::clone(&data_home);
            let pinned_file = Rc::clone(&pinned_file);
            let per_monitor = Rc::clone(&per_monitor);
            let holder = Rc::clone(&rebuild_holder);

            let rebuild_fn = Rc::new(move || {
                let self_ref = holder.borrow().clone().unwrap_or_else(|| Rc::new(|| {}));
                for (alignment_box, current, win) in per_monitor.borrow().iter() {
                    if let Some(old) = current.borrow_mut().take() {
                        alignment_box.remove(&old);
                    }
                    let new_box = ui::dock_box::build(
                        alignment_box, &config, &state, &data_home,
                        &pinned_file, &self_ref, win,
                    );
                    *current.borrow_mut() = Some(new_box);
                }
            });

            *rebuild_holder.borrow_mut() = Some(rebuild_fn);
        }

        // Initial build + present all windows
        let rebuild = rebuild_holder.borrow().clone().unwrap();
        rebuild();

        for win in all_windows.borrow().iter() {
            win.present();
        }

        // Autohide: hide after initial show, start cursor edge poller
        if config.autohide {
            for win in all_windows.borrow().iter() {
                let win_hide = win.clone();
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(500),
                    move || { win_hide.set_visible(false); },
                );
            }

            // Use Hyprland IPC cursor polling instead of GTK hotspot windows
            ui::hotspot::start_cursor_poller(&all_windows, &config, &state);
        }

        // Hyprland event listener — rebuilds all monitors
        let rebuild_for_events = rebuild_holder.borrow().clone().unwrap();
        events::start_event_listener(Rc::clone(&state), rebuild_for_events);

        // Pin file watcher — instant rebuild when pins change (e.g. from drawer)
        {
            let pin_path = pinned_file.as_ref().clone();
            let rebuild_pin = rebuild_holder.borrow().clone().unwrap();
            let (pin_tx, pin_rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                use notify::{Watcher, RecursiveMode};
                let tx = pin_tx;
                let mut watcher = match notify::recommended_watcher(
                    move |res: Result<notify::Event, _>| {
                        if let Ok(event) = res
                            && matches!(event.kind,
                                notify::EventKind::Modify(_) |
                                notify::EventKind::Create(_)
                            ) {
                                let _ = tx.send(());
                            }
                    },
                ) {
                    Ok(w) => w,
                    Err(e) => { log::warn!("Pin watcher failed: {}", e); return; }
                };
                // Watch the parent directory to catch file creation
                if let Some(parent) = pin_path.parent() {
                    let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
                }
                // Block thread forever (watcher dropped = stops watching)
                std::thread::park();
            });

            glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                if pin_rx.try_recv().is_ok() {
                    // Drain any extra notifications
                    while pin_rx.try_recv().is_ok() {}
                    log::debug!("Pin file changed, rebuilding dock");
                    rebuild_pin();
                }
                glib::ControlFlow::Continue
            });
        }

        // Signal handler — controls all windows
        let all_win_sig = Rc::clone(&all_windows);
        let sig_rx = Rc::clone(&sig_rx);
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            while let Ok(cmd) = sig_rx.try_recv() {
                for win in all_win_sig.borrow().iter() {
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
    });

    app.run_with_args::<String>(&[]);
}
