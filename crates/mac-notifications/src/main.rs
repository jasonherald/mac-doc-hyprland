mod config;
mod dbus;
mod notification;
mod state;
mod ui;

use crate::config::NotificationConfig;
use crate::state::NotificationState;
use crate::ui::popup::PopupManager;
use clap::Parser;
use dock_common::desktop::dirs::get_app_dirs;
use dock_common::singleton;
use gtk4::gio;
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

    // Keep app alive (no visible windows initially)
    let hold_guard: Rc<RefCell<Option<gio::ApplicationHoldGuard>>> = Rc::new(RefCell::new(None));
    let hold_ref = Rc::clone(&hold_guard);

    app.connect_activate(move |app| {
        *hold_ref.borrow_mut() = Some(app.hold());

        // CSS
        ui::css::load_notification_css();

        // State
        let app_dirs = get_app_dirs();
        let state = Rc::new(RefCell::new(NotificationState::new(
            app_dirs,
            config.max_history,
        )));
        state.borrow_mut().dnd = config.dnd;

        // Popup manager
        let popup_mgr = Rc::new(RefCell::new(PopupManager::new(app, &config)));

        // D-Bus callbacks
        let state_notify = Rc::clone(&state);
        let popup_mgr_notify = Rc::clone(&popup_mgr);
        let on_notify: dbus::OnNotify = Rc::new(move |notif| {
            log::info!("[{}] {}: {}", notif.app_name, notif.summary, notif.body);

            // Show popup if not suppressed by DND
            if state_notify.borrow().should_show_popup(notif.urgency) {
                popup_mgr_notify.borrow_mut().show(notif, &state_notify);
            }
        });

        let on_close: dbus::OnClose = Rc::new(move |id| {
            log::debug!("Notification {} closed via D-Bus", id);
        });

        // Register D-Bus server
        dbus::register_server(&state, on_notify, on_close);

        log::info!("Notification daemon started");
    });

    app.run_with_args::<String>(&[]);
}
