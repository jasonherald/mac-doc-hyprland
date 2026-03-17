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
use gtk4::prelude::*;
use nwg_dock_common::config::paths;
use nwg_dock_common::desktop::dirs::get_app_dirs;
use nwg_dock_common::pinning;
use nwg_dock_common::signals;
use nwg_dock_common::singleton;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Detects and creates the compositor backend, exiting on failure.
fn init_compositor(wm: &str) -> Rc<dyn nwg_dock_common::compositor::Compositor> {
    let wm_override = if wm.is_empty() { None } else { Some(wm) };
    let compositor_kind = match nwg_dock_common::compositor::detect(wm_override) {
        Ok(k) => k,
        Err(e) => {
            log::error!("{}", e);
            std::process::exit(1);
        }
    };
    match nwg_dock_common::compositor::create(compositor_kind) {
        Ok(c) => Rc::from(c),
        Err(e) => {
            log::error!("{}", e);
            std::process::exit(1);
        }
    }
}

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

    auto_detect_launcher(&mut config);
    let compositor = init_compositor(&config.wm);
    let _lock = acquire_singleton_lock("mac-dock", config.multi, config.is_resident_mode());

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
        activate_dock(
            app,
            &css_path,
            &config,
            &app_dirs,
            &compositor,
            &pinned_file,
            &data_home,
            &sig_rx,
        );
    });

    app.run_with_args::<String>(&[]);
}

/// Sets up the dock UI: state, monitors, windows, rebuild function, and listeners.
#[allow(clippy::too_many_arguments)]
fn activate_dock(
    app: &gtk4::Application,
    css_path: &Rc<std::path::PathBuf>,
    config: &Rc<DockConfig>,
    app_dirs: &[std::path::PathBuf],
    compositor: &Rc<dyn nwg_dock_common::compositor::Compositor>,
    pinned_file: &Rc<std::path::PathBuf>,
    data_home: &Rc<std::path::PathBuf>,
    sig_rx: &Rc<std::sync::mpsc::Receiver<signals::WindowCommand>>,
) {
    ui::css::load_dock_css(css_path);
    let _hold = app.hold();

    let state = Rc::new(RefCell::new(DockState::new(
        app_dirs.to_vec(),
        Rc::clone(compositor),
    )));
    state.borrow_mut().pinned = pinning::load_pinned(pinned_file);
    state.borrow_mut().locked = ui::dock_menu::load_lock_state();
    if let Err(e) = state.borrow_mut().refresh_clients() {
        log::error!("Couldn't list clients: {}", e);
    }

    let monitors = monitor::resolve_monitors(&state, config);

    let (per_monitor, all_windows) = dock_windows::create_dock_windows(app, &monitors, config);
    let per_monitor = Rc::new(RefCell::new(per_monitor));

    let rebuild = rebuild::create_rebuild_fn(
        &per_monitor,
        config,
        &state,
        data_home,
        pinned_file,
        compositor,
    );
    rebuild();

    for win in all_windows.borrow().iter() {
        win.present();
    }

    if config.autohide {
        listeners::setup_autohide(&all_windows, config, &state, compositor, app, &monitors);
    }
    events::start_event_listener(
        Rc::clone(&state),
        Rc::clone(&rebuild),
        Rc::clone(compositor),
    );
    listeners::setup_pin_watcher(pinned_file, &rebuild);
    listeners::setup_signal_poller(&all_windows, sig_rx);
}

/// Auto-detect launcher: hide button if command not found on PATH.
fn auto_detect_launcher(config: &mut DockConfig) {
    if config.nolauncher || config.launcher_cmd.is_empty() {
        return;
    }
    let cmd = config.launcher_cmd.split_whitespace().next().unwrap_or("");
    if !cmd.is_empty() && !command_exists(cmd) {
        log::info!(
            "Launcher command '{}' not found on PATH, hiding launcher",
            cmd
        );
        config.nolauncher = true;
    }
}

/// Acquires the singleton lock, sending toggle to existing instance if needed.
fn acquire_singleton_lock(
    app_name: &str,
    multi: bool,
    is_resident: bool,
) -> Option<singleton::LockFile> {
    if multi {
        return None;
    }
    match singleton::acquire_lock(app_name) {
        Ok(lock) => Some(lock),
        Err(existing_pid) => {
            if let Some(pid) = existing_pid {
                if is_resident {
                    log::info!("Running instance found (pid {}), terminating...", pid);
                } else {
                    signals::send_signal_to_pid(pid, signals::sig_toggle());
                    log::info!("Sent toggle signal to running instance (pid {}), bye!", pid);
                }
            }
            std::process::exit(0);
        }
    }
}

/// Checks if a command exists on PATH.
fn command_exists(cmd: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let full = std::path::Path::new(dir).join(cmd);
            if full.is_file() {
                return true;
            }
        }
    }
    false
}
