mod config;
mod dbus;
mod notification;
mod state;

use crate::config::NotificationConfig;
use crate::state::NotificationState;
use clap::Parser;
use dock_common::desktop::dirs::get_app_dirs;
use dock_common::singleton;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    let config = NotificationConfig::parse();

    if config.debug {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::init();
    }

    let _lock = match singleton::acquire_lock("mac-notifications") {
        Ok(lock) => lock,
        Err(existing_pid) => {
            if let Some(pid) = existing_pid {
                log::info!("Already running (pid {})", pid);
            }
            std::process::exit(0);
        }
    };

    let app = gtk4::Application::builder()
        .application_id("com.mac-notifications.hyprland")
        .build();

    let config = Rc::new(config);

    // Hold the application so it doesn't quit (no visible windows initially)
    let hold_guard: Rc<RefCell<Option<gtk4::gio::ApplicationHoldGuard>>> =
        Rc::new(RefCell::new(None));
    let hold_ref = Rc::clone(&hold_guard);

    app.connect_activate(move |_app| {
        *hold_ref.borrow_mut() = Some(_app.hold());

        // State
        let app_dirs = get_app_dirs();
        let state = Rc::new(RefCell::new(NotificationState::new(
            app_dirs,
            config.max_history,
        )));
        state.borrow_mut().dnd = config.dnd;

        // D-Bus callbacks
        let on_notify: dbus::OnNotify = Rc::new(|notif| {
            log::info!("[{}] {}: {}", notif.app_name, notif.summary, notif.body);
            // TODO Phase 2: show popup
            // TODO Phase 4: update waybar
        });

        let on_close: dbus::OnClose = Rc::new(|id| {
            log::debug!("Notification {} closed via D-Bus", id);
        });

        // Register D-Bus server
        dbus::register_server(&state, on_notify, on_close);

        log::info!("Notification daemon started");
    });

    app.run_with_args::<String>(&[]);
}
