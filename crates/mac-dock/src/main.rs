mod config;
mod context;
mod dock_windows;
mod events;
mod listeners;
mod monitor;
mod rebuild;
mod state;
mod ui;

use crate::config::DockConfig;
use crate::state::DockState;
use clap::Parser;
use dock_common::config::paths;
use dock_common::desktop::dirs::get_app_dirs;
use dock_common::pinning;
use dock_common::signals;
use dock_common::singleton;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

fn main() {
    let mut config = DockConfig::parse();

    if config.debug {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::init();
    }

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

    let config_dir = paths::config_dir("nwg-dock-hyprland");
    if let Err(e) = paths::ensure_dir(&config_dir) {
        log::warn!("Failed to create config dir: {}", e);
    }

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
    let css_path = Rc::new(css_path);

    app.connect_activate(move |app| {
        ui::css::load_dock_css(&css_path);
        let _hold = app.hold();

        // State
        let state = Rc::new(RefCell::new(DockState::new(app_dirs.clone())));
        state.borrow_mut().pinned = pinning::load_pinned(&pinned_file);
        state.borrow_mut().locked = ui::dock_menu::load_lock_state();
        if let Err(e) = state.borrow_mut().refresh_clients() {
            log::error!("Couldn't list clients: {}", e);
        }

        // Monitors
        let monitors = monitor::resolve_monitors(&state, &config);

        // Windows
        let (per_monitor, all_windows) = dock_windows::create_dock_windows(app, &monitors, &config);
        let per_monitor = Rc::new(RefCell::new(per_monitor));

        // Rebuild function
        let rebuild =
            rebuild::create_rebuild_fn(&per_monitor, &config, &state, &data_home, &pinned_file);
        rebuild();

        for win in all_windows.borrow().iter() {
            win.present();
        }

        // Listeners
        if config.autohide {
            listeners::setup_autohide(&all_windows, &config, &state);
        }
        events::start_event_listener(Rc::clone(&state), Rc::clone(&rebuild));
        listeners::setup_pin_watcher(&pinned_file, &rebuild);
        listeners::setup_signal_poller(&all_windows, &sig_rx);
    });

    app.run_with_args::<String>(&[]);
}
