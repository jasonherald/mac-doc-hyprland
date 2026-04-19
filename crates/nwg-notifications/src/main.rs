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
use nwg_common::desktop::dirs::get_app_dirs;
use nwg_common::singleton;
use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    nwg_common::process::handle_dump_args();
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

    let compositor: Rc<dyn nwg_common::compositor::Compositor> =
        Rc::from(nwg_common::compositor::init_or_exit(config.wm));

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
        activate_notifications(app, &config, &compositor, &sig_rx);
    });

    app.run_with_args::<String>(&[]);
}

/// Sets up the notification daemon: state, popup manager, panel, D-Bus server, and listeners.
fn activate_notifications(
    app: &gtk4::Application,
    config: &Rc<NotificationConfig>,
    compositor: &Rc<dyn nwg_common::compositor::Compositor>,
    sig_rx: &Rc<std::sync::mpsc::Receiver<listeners::NotificationCommand>>,
) {
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
            s.history.sort_by_key(|n| std::cmp::Reverse(n.timestamp));
            let max = s.max_history;
            s.history.truncate(max);
        }
    }

    // Write initial waybar status
    let s = state.borrow();
    waybar::update_status(s.unread_count(), s.dnd);
    drop(s);

    // Shared callback for any state change -> save history + update waybar
    let on_state_change = build_state_change_callback(&state, config.persist, history_path);

    // Popup manager
    let popup_mgr = Rc::new(RefCell::new(PopupManager::new(
        app,
        config,
        Rc::clone(&on_state_change),
        Rc::clone(compositor),
    )));

    // Panel
    let on_panel_click = build_panel_click_callback(&state, compositor);
    let panel = Rc::new(RefCell::new(NotificationPanel::new(
        app,
        &state,
        on_panel_click,
        Rc::clone(&on_state_change),
    )));

    // D-Bus callbacks
    let on_notify = build_on_notify_callback(&state, &popup_mgr, &panel, &on_state_change);
    let on_change_close = Rc::clone(&on_state_change);
    let popup_mgr_close = Rc::clone(&popup_mgr);
    let on_close: dbus::OnClose = Rc::new(move |id| {
        log::debug!("Notification {} closed via D-Bus", id);
        popup_mgr_close.borrow_mut().dismiss(id);
        on_change_close();
    });

    dbus::register_server(&state, on_notify, on_close);

    // DND menu (right-click waybar bell)
    let dnd_menu = Rc::new(RefCell::new(ui::dnd_menu::DndMenu::new(
        app,
        &state,
        Rc::clone(&on_state_change),
    )));

    listeners::poll_signals(sig_rx, &panel, &state, &on_state_change, &dnd_menu);

    log::info!(
        "Notification daemon started (panel: SIGRTMIN+4, DND: SIGRTMIN+5, menu: SIGRTMIN+6)"
    );
}

/// Creates the shared on_state_change callback that persists history and updates waybar.
fn build_state_change_callback(
    state: &Rc<RefCell<NotificationState>>,
    persist: bool,
    history_path: std::path::PathBuf,
) -> Rc<dyn Fn()> {
    let state_sync = Rc::clone(state);
    Rc::new(move || {
        let s = state_sync.borrow();
        waybar::update_status(s.unread_count(), s.dnd);
        if persist {
            persistence::save_history(&history_path, &s.history);
        }
    })
}

/// Creates the callback invoked when a notification row is clicked in the panel.
fn build_panel_click_callback(
    state: &Rc<RefCell<NotificationState>>,
    compositor: &Rc<dyn nwg_common::compositor::Compositor>,
) -> Rc<dyn Fn(u32)> {
    let state_click = Rc::clone(state);
    let compositor_panel = Rc::clone(compositor);
    Rc::new(move |id| {
        let s = state_click.borrow();
        if let Some(notif) = s.history.iter().find(|n| n.id == id) {
            let app_name = notif.app_name.clone();
            let desktop_entry = notif.desktop_entry.clone();
            drop(s);
            ui::popup::focus_app(
                &app_name,
                desktop_entry.as_deref(),
                &state_click,
                &*compositor_panel,
            );
            state_click.borrow_mut().mark_read(id);
        }
    })
}

/// Creates the D-Bus on_notify callback that shows popups and refreshes the panel.
fn build_on_notify_callback(
    state: &Rc<RefCell<NotificationState>>,
    popup_mgr: &Rc<RefCell<PopupManager>>,
    panel: &Rc<RefCell<NotificationPanel>>,
    on_state_change: &Rc<dyn Fn()>,
) -> dbus::OnNotify {
    let state_notify = Rc::clone(state);
    let popup_mgr_notify = Rc::clone(popup_mgr);
    let panel_notify = Rc::clone(panel);
    let on_change_notify = Rc::clone(on_state_change);
    Rc::new(move |notif| {
        log::info!("[{}] {}: {}", notif.app_name, notif.summary, notif.body);

        if state_notify.borrow().should_show_popup(notif.urgency) {
            popup_mgr_notify.borrow_mut().show(notif, &state_notify);
        }

        if panel_notify.borrow().is_visible() {
            panel_notify.borrow().rebuild();
        }

        on_change_notify();
    })
}
