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

    // Logging
    if config.debug {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::init();
    }

    // Validate mutually exclusive flags
    let mut config = config;
    if config.autohide && config.resident {
        log::warn!("autohide and resident are mutually exclusive, ignoring -d!");
        config.autohide = false;
    }

    // Verify Hyprland is running
    if dock_common::hyprland::ipc::instance_signature().is_err() {
        log::error!("HYPRLAND_INSTANCE_SIGNATURE not found, terminating.");
        std::process::exit(1);
    }

    // Single instance check
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

    // Resolve data directories
    let data_home = paths::find_data_home("nwg-dock-hyprland").unwrap_or_else(|| {
        log::error!("No data directory found for nwg-dock-hyprland");
        PathBuf::from("/usr/share")
    });

    let config_dir = paths::config_dir("nwg-dock-hyprland");
    paths::ensure_dir(&config_dir);

    // Copy default CSS if missing
    let css_path = config_dir.join(&config.css_file);
    if !css_path.exists() {
        let src = data_home.join("nwg-dock-hyprland/style.css");
        if let Err(e) = paths::copy_file(&src, &css_path) {
            log::warn!("Error copying default CSS: {}", e);
        }
    }

    // Pinned file
    let cache_dir = paths::cache_dir().expect("Couldn't determine cache directory");
    let pinned_file = cache_dir.join("nwg-dock-pinned");

    // App directories
    let app_dirs = get_app_dirs();

    // Signal handler
    let sig_rx = Rc::new(signals::setup_signal_handlers(config.is_resident_mode()));

    // Build GTK application
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
        let css_path = Rc::clone(&css_path_rc);

        // Load CSS
        ui::css::load_dock_css(&css_path);

        // Create state
        let state = Rc::new(RefCell::new(DockState::new(app_dirs.clone())));

        // Load initial pinned items
        state.borrow_mut().pinned = pinning::load_pinned(&pinned_file);

        // Create main window
        let win = gtk4::ApplicationWindow::new(app);
        ui::window::setup_dock_window(&win, &config);

        // Map outputs for monitor targeting
        let output_map = monitor::map_outputs(&state);
        if !config.output.is_empty() {
            if let Some(mon) = output_map.get(&config.output) {
                win.set_monitor(Some(mon));
            } else {
                log::warn!("Target output '{}' not found, ignoring", config.output);
            }
        }

        // Outer container
        let (outer_orient, _inner_orient) = ui::window::orientations(&config);
        let outer_box = gtk4::Box::new(outer_orient, 0);
        outer_box.set_widget_name("box");
        win.set_child(Some(&outer_box));

        let inner_orient = if config.is_vertical() {
            gtk4::Orientation::Vertical
        } else {
            gtk4::Orientation::Horizontal
        };
        let alignment_box = gtk4::Box::new(inner_orient, 0);
        alignment_box.set_hexpand(true);
        alignment_box.set_vexpand(true);
        outer_box.append(&alignment_box);

        // Initial client list
        if let Err(e) = state.borrow_mut().refresh_clients() {
            log::error!("Couldn't list clients: {}", e);
        }

        // Build main box (the dock content)
        // Use shared holder so rebuild can reference itself
        type RebuildHolder = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
        let rebuild_holder: RebuildHolder = Rc::new(RefCell::new(None));
        let current_main_box: Rc<RefCell<Option<gtk4::Box>>> = Rc::new(RefCell::new(None));

        {
            let alignment_box = alignment_box.clone();
            let config = Rc::clone(&config);
            let state = Rc::clone(&state);
            let data_home = Rc::clone(&data_home);
            let current = Rc::clone(&current_main_box);
            let pinned_file_ref = Rc::clone(&pinned_file);
            let win_ref = win.clone();
            let holder = Rc::clone(&rebuild_holder);

            let rebuild_fn = Rc::new(move || {
                if let Some(old) = current.borrow_mut().take() {
                    alignment_box.remove(&old);
                }
                // Get self-reference for passing to buttons
                let self_ref = holder.borrow().clone().unwrap_or_else(|| Rc::new(|| {}));
                let new_box = ui::dock_box::build(
                    &alignment_box,
                    &config,
                    &state,
                    &data_home,
                    &pinned_file_ref,
                    &self_ref,
                    &win_ref,
                );
                *current.borrow_mut() = Some(new_box);
            });

            *rebuild_holder.borrow_mut() = Some(rebuild_fn);
        }

        // Initial build
        let rebuild = rebuild_holder.borrow().clone().unwrap();
        rebuild();

        win.present();

        // Autohide: hide after initial show, set up hotspot
        if config.autohide {
            let win_hide = win.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
                win_hide.set_visible(false);
            });

            // Load hotspot CSS
            let hotspot_css_path = paths::config_dir("nwg-dock-hyprland").join("hotspot.css");
            let _hotspot_provider = ui::css::load_hotspot_css(&hotspot_css_path);

            // Create hotspots
            if config.output.is_empty() {
                for mon in monitor::list_gdk_monitors() {
                    let hotspot_win = ui::hotspot::setup_hotspot(
                        &mon, &win, &config, &state, app,
                    );
                    hotspot_win.present();
                }
            } else if let Some(mon) = output_map.get(&config.output) {
                let hotspot_win = ui::hotspot::setup_hotspot(
                    mon, &win, &config, &state, app,
                );
                hotspot_win.present();
            }
        }

        // Dock leave → autohide timeout
        if config.autohide {
            let win_leave = win.clone();
            let state_leave = Rc::clone(&state);
            let leave_ctrl = gtk4::EventControllerMotion::new();
            leave_ctrl.connect_leave(move |_| {
                let win_ref = win_leave.clone();
                let state_ref = Rc::clone(&state_leave);
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(1000),
                    move || {
                        state_ref.borrow_mut().mouse_inside_dock = false;
                        let s = state_ref.borrow();
                        if !s.mouse_inside_dock && !s.mouse_inside_hotspot {
                            win_ref.set_visible(false);
                        }
                    },
                );
            });
            win.add_controller(leave_ctrl);

            let state_enter = Rc::clone(&state);
            let enter_ctrl = gtk4::EventControllerMotion::new();
            enter_ctrl.connect_enter(move |_, _, _| {
                state_enter.borrow_mut().mouse_inside_dock = true;
            });
            win.add_controller(enter_ctrl);
        }

        // Start Hyprland event listener
        let rebuild_for_events = rebuild_holder.borrow().clone().unwrap();
        events::start_event_listener(Rc::clone(&state), rebuild_for_events);

        // Signal handler polling (check for show/hide/toggle signals)
        let win_sig = win.clone();
        let sig_rx = Rc::clone(&sig_rx);
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            while let Ok(cmd) = sig_rx.try_recv() {
                match cmd {
                    WindowCommand::Show => {
                        if !win_sig.is_visible() {
                            win_sig.set_visible(true);
                        }
                    }
                    WindowCommand::Hide => {
                        if win_sig.is_visible() {
                            win_sig.set_visible(false);
                        }
                    }
                    WindowCommand::Toggle => {
                        win_sig.set_visible(!win_sig.is_visible());
                    }
                    WindowCommand::Quit => {
                        win_sig.close();
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    });

    app.run_with_args::<String>(&[]);
}
