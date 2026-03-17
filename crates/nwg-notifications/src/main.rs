mod config;
mod dbus;
mod listeners;
mod notification;
mod persistence;
mod state;
mod ui;
mod waybar;

use crate::config::NotificationConfig;
use crate::state::NotificationState;
use crate::ui::panel::NotificationPanel;
use crate::ui::popup::PopupManager;
use clap::Parser;
use gtk4::gio;
use gtk4::prelude::*;
use nwg_dock_common::desktop::dirs::get_app_dirs;
use nwg_dock_common::singleton;
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

    let compositor = init_compositor(&config.wm);

    // Signal listener — BEFORE GTK, same pattern as the dock
    let sig_rx = listeners::start_signal_listener();

    let app = gtk4::Application::builder()
        .application_id("com.mac-notifications.hyprland")
        .build();

    let config = Rc::new(config);
    let hold_guard: Rc<RefCell<Option<gio::ApplicationHoldGuard>>> = Rc::new(RefCell::new(None));
    let hold_ref = Rc::clone(&hold_guard);

    app.connect_activate(move |app| {
        *hold_ref.borrow_mut() = Some(app.hold());
        ui::css::load_notification_css();

        // State
        let app_dirs = get_app_dirs();
        let state = Rc::new(RefCell::new(NotificationState::new(
            app_dirs,
            config.max_history,
        )));
        state.borrow_mut().dnd = config.dnd;

        // Load persisted history
        let history_path = persistence::history_path();
        if config.persist {
            let loaded = persistence::load_history(&history_path);
            if !loaded.is_empty() {
                log::info!("Loaded {} notifications from history", loaded.len());
                let mut s = state.borrow_mut();
                for notif in loaded {
                    s.history.push(notif);
                }
                // Ensure newest-first ordering
                s.history.sort_by_key(|n| std::cmp::Reverse(n.timestamp));
            }
        }

        // Write initial waybar status
        let s = state.borrow();
        waybar::update_status(s.unread_count(), s.dnd);
        drop(s);

        // Shared callback for any state change → save history + update waybar
        let state_sync = Rc::clone(&state);
        let persist = config.persist;
        let sync_path = history_path;
        let on_state_change: Rc<dyn Fn()> = Rc::new(move || {
            let s = state_sync.borrow();
            waybar::update_status(s.unread_count(), s.dnd);
            if persist {
                persistence::save_history(&sync_path, &s.history);
            }
        });

        // Popup manager
        let popup_mgr = Rc::new(RefCell::new(PopupManager::new(
            app,
            &config,
            Rc::clone(&on_state_change),
            Rc::clone(&compositor),
        )));

        // Panel
        let state_panel_click = Rc::clone(&state);
        let compositor_panel = Rc::clone(&compositor);
        let on_panel_click: Rc<dyn Fn(u32)> = Rc::new(move |id| {
            let s = state_panel_click.borrow();
            if let Some(notif) = s.history.iter().find(|n| n.id == id) {
                let app_name = notif.app_name.clone();
                let desktop_entry = notif.desktop_entry.clone();
                drop(s);
                ui::popup::focus_app(
                    &app_name,
                    desktop_entry.as_deref(),
                    &state_panel_click,
                    &*compositor_panel,
                );
                state_panel_click.borrow_mut().mark_read(id);
            }
        });

        let panel = Rc::new(RefCell::new(NotificationPanel::new(
            app,
            &state,
            on_panel_click,
            Rc::clone(&on_state_change),
        )));

        // D-Bus callbacks
        let state_notify = Rc::clone(&state);
        let popup_mgr_notify = Rc::clone(&popup_mgr);
        let panel_notify = Rc::clone(&panel);
        let on_change_notify = Rc::clone(&on_state_change);
        let on_notify: dbus::OnNotify = Rc::new(move |notif| {
            log::info!("[{}] {}: {}", notif.app_name, notif.summary, notif.body);

            if state_notify.borrow().should_show_popup(notif.urgency) {
                popup_mgr_notify.borrow_mut().show(notif, &state_notify);
            }

            if panel_notify.borrow().is_visible() {
                panel_notify.borrow().rebuild();
            }

            on_change_notify();
        });

        let on_change_close = Rc::clone(&on_state_change);
        let on_close: dbus::OnClose = Rc::new(move |id| {
            log::debug!("Notification {} closed via D-Bus", id);
            on_change_close();
        });

        dbus::register_server(&state, on_notify, on_close);

        // DND menu (right-click waybar bell)
        let dnd_menu = Rc::new(RefCell::new(ui::dnd_menu::DndMenu::new(
            app,
            &state,
            Rc::clone(&on_state_change),
        )));

        // Poll signal receiver on GTK main thread
        listeners::poll_signals(&sig_rx, &panel, &state, &on_state_change, &dnd_menu);

        log::info!(
            "Notification daemon started (panel: SIGRTMIN+4, DND: SIGRTMIN+5, menu: SIGRTMIN+6)"
        );
    });

    app.run_with_args::<String>(&[]);
}

/// Detects the compositor kind and creates the compositor instance.
/// Exits with an error if detection or creation fails.
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
