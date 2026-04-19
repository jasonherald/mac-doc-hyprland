use super::constants::{
    POPUP_BODY_CHARS, POPUP_GAP, POPUP_ICON_SIZE, POPUP_MAX_BODY_LINES, POPUP_PADDING,
    POPUP_SUMMARY_CHARS, POPUP_TOP_MARGIN, POPUP_WIDTH,
};
use super::window;
use crate::config::NotificationConfig;
use crate::notification::{Notification, Urgency};
use crate::state::NotificationState;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use nwg_common::compositor::Compositor;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Tracks an active popup window and its notification ID.
struct ActivePopup {
    id: u32,
    win: gtk4::ApplicationWindow,
}

/// Manages popup notification windows.
pub struct PopupManager {
    popups: Vec<ActivePopup>,
    config: Rc<NotificationConfig>,
    app: gtk4::Application,
    on_state_change: Rc<dyn Fn()>,
    compositor: Rc<dyn Compositor>,
}

impl PopupManager {
    pub fn new(
        app: &gtk4::Application,
        config: &Rc<NotificationConfig>,
        on_state_change: Rc<dyn Fn()>,
        compositor: Rc<dyn Compositor>,
    ) -> Self {
        Self {
            popups: Vec::new(),
            config: Rc::clone(config),
            app: app.clone(),
            on_state_change,
            compositor,
        }
    }

    /// Shows a popup for a notification. Respects max_popups limit.
    pub fn show(&mut self, notif: &Notification, state: &Rc<RefCell<NotificationState>>) {
        // Remove stale entries (windows closed by timer or click)
        self.popups.retain(|p| p.win.is_visible());

        while self.popups.len() >= self.config.max_popups {
            if let Some(old) = self.popups.first() {
                let old_id = old.id;
                self.dismiss(old_id);
            } else {
                break;
            }
        }

        let top_offset = self.calculate_offset();
        let win = gtk4::ApplicationWindow::new(&self.app);
        window::setup_popup_window(&win, self.config.popup_position, top_offset);
        win.add_css_class("notification-popup-window");
        win.set_width_request(POPUP_WIDTH);
        win.set_default_size(POPUP_WIDTH, -1);

        // Show on the focused monitor
        if let Some(mon) = focused_gdk_monitor(&*self.compositor) {
            win.set_monitor(Some(&mon));
        }

        let content = build_popup_content(notif, &state.borrow().app_dirs, state, &win);
        win.set_child(Some(&content));

        // Click anywhere on popup → focus app + dismiss popup
        let notif_app = notif.app_name.clone();
        let notif_desktop = notif.desktop_entry.clone();
        let notif_id = notif.id;
        let has_default_action = notif.actions.iter().any(|(key, _)| key == "default");
        let state_click = Rc::clone(state);
        let win_click = win.clone();
        let on_change_click = Rc::clone(&self.on_state_change);
        let compositor_click = Rc::clone(&self.compositor);
        let click = gtk4::GestureClick::new();
        click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);

            // Emit ActionInvoked with "default" action so the app can deep-link
            // to the specific item (mail message, chat thread, etc.)
            if has_default_action {
                let s = state_click.borrow();
                if let Some(conn) = &s.dbus_connection {
                    crate::dbus::emit_action_invoked(conn, notif_id, "default");
                }
                drop(s);
            }

            focus_app(
                &notif_app,
                notif_desktop.as_deref(),
                &state_click,
                &*compositor_click,
            );
            state_click.borrow_mut().mark_read(notif_id);
            state_click.borrow_mut().active_popups.remove(&notif_id);
            win_click.close();
            on_change_click();
        });
        win.add_controller(click);

        win.present();

        let id = notif.id;
        self.popups.push(ActivePopup {
            id,
            win: win.clone(),
        });
        state.borrow_mut().active_popups.insert(id);

        // Auto-dismiss timer
        let timeout = self.resolve_timeout(notif);
        if timeout > 0 {
            let state_timer = Rc::clone(state);
            let win_timer = win;
            let on_change_timer = Rc::clone(&self.on_state_change);
            gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_millis(timeout),
                move || {
                    state_timer.borrow_mut().active_popups.remove(&id);
                    // Don't mark_read here — auto-dismiss doesn't mean the user saw it.
                    // Only explicit clicks mark notifications as read.
                    win_timer.close();
                    on_change_timer();
                },
            );
        }
    }

    /// Dismisses a popup by notification ID.
    pub fn dismiss(&mut self, id: u32) {
        if let Some(pos) = self.popups.iter().position(|p| p.id == id) {
            let popup = self.popups.remove(pos);
            popup.win.close();
            self.restack();
        }
    }

    /// Recalculates top margins for all popups after one is removed.
    fn restack(&self) {
        for (i, popup) in self.popups.iter().enumerate() {
            let offset = POPUP_TOP_MARGIN + (i as i32) * (self.estimated_height() + POPUP_GAP);
            let is_top = matches!(
                self.config.popup_position,
                crate::config::PopupPosition::TopRight | crate::config::PopupPosition::TopLeft
            );
            if is_top {
                popup.win.set_margin(gtk4_layer_shell::Edge::Top, offset);
            } else {
                popup.win.set_margin(gtk4_layer_shell::Edge::Bottom, offset);
            }
        }
    }

    fn calculate_offset(&self) -> i32 {
        POPUP_TOP_MARGIN + (self.popups.len() as i32) * (self.estimated_height() + POPUP_GAP)
    }

    fn estimated_height(&self) -> i32 {
        POPUP_ICON_SIZE + POPUP_PADDING
    }

    fn resolve_timeout(&self, notif: &Notification) -> u64 {
        if notif.urgency == Urgency::Critical {
            return 0; // never auto-dismiss critical
        }
        if notif.timeout_ms > 0 {
            notif.timeout_ms as u64
        } else {
            self.config.popup_timeout
        }
    }
}

/// Builds the popup widget content: icon + text + optional action buttons.
fn build_popup_content(
    notif: &Notification,
    app_dirs: &[PathBuf],
    state: &Rc<RefCell<NotificationState>>,
    win: &gtk4::ApplicationWindow,
) -> gtk4::Box {
    let outer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    outer.add_css_class("notification-popup");

    if notif.urgency == Urgency::Critical {
        outer.add_css_class("urgency-critical");
    }

    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);

    let icon = super::icons::resolve_popup_icon(
        &notif.app_icon,
        &notif.app_name,
        notif.desktop_entry.as_deref(),
        app_dirs,
        POPUP_ICON_SIZE,
    );
    icon.add_css_class("popup-icon");
    container.append(&icon);

    let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    text_box.set_hexpand(true);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let app_label = gtk4::Label::new(Some(&notif.app_name));
    app_label.add_css_class("popup-app-name");
    app_label.set_halign(gtk4::Align::Start);
    app_label.set_hexpand(true);
    app_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    header.append(&app_label);

    let time_label = gtk4::Label::new(Some("now"));
    time_label.add_css_class("popup-time");
    header.append(&time_label);
    text_box.append(&header);

    let summary = gtk4::Label::new(Some(&notif.summary));
    summary.add_css_class("popup-summary");
    summary.set_halign(gtk4::Align::Start);
    summary.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    summary.set_max_width_chars(POPUP_SUMMARY_CHARS);
    summary.set_wrap(true);
    summary.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    summary.set_lines(1);
    text_box.append(&summary);

    if !notif.body.is_empty() {
        let body = gtk4::Label::new(Some(&notif.body));
        body.add_css_class("popup-body");
        body.set_halign(gtk4::Align::Start);
        body.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        body.set_max_width_chars(POPUP_BODY_CHARS);
        body.set_lines(POPUP_MAX_BODY_LINES);
        body.set_wrap(true);
        body.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        text_box.append(&body);
    }

    container.append(&text_box);
    outer.append(&container);

    // Action buttons (e.g. Reply, Open, etc.)
    let actions: Vec<_> = notif
        .actions
        .iter()
        .filter(|(key, _)| key != "default")
        .cloned()
        .collect();

    if !actions.is_empty() {
        let action_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        action_box.add_css_class("popup-actions");
        action_box.set_halign(gtk4::Align::End);
        action_box.set_margin_top(6);

        let notif_id = notif.id;
        for (action_key, action_label) in actions {
            let btn = gtk4::Button::with_label(&action_label);
            btn.add_css_class("popup-action-btn");

            let state_action = Rc::clone(state);
            let key = action_key.clone();
            let win_action = win.clone();
            btn.connect_clicked(move |_| {
                let s = state_action.borrow();
                if let Some(conn) = &s.dbus_connection {
                    crate::dbus::emit_action_invoked(conn, notif_id, &key);
                }
                drop(s);
                state_action.borrow_mut().active_popups.remove(&notif_id);
                win_action.close();
            });
            action_box.append(&btn);
        }
        outer.append(&action_box);
    }

    outer
}

/// Finds the GDK monitor that the compositor reports as focused.
fn focused_gdk_monitor(compositor: &dyn Compositor) -> Option<gtk4::gdk::Monitor> {
    let wm_monitors = compositor.list_monitors().ok()?;
    let focused_idx = wm_monitors.iter().position(|m| m.focused)?;

    let display = gtk4::gdk::Display::default()?;
    let monitors = display.monitors();
    let item = monitors.item(focused_idx as u32)?;
    item.downcast::<gtk4::gdk::Monitor>().ok()
}

/// Attempts to focus the app that sent the notification.
///
/// Matches by: exact class, class contains app_name, or app_name contains class.
/// This handles cases like app_name="Brave" matching class="brave-browser".
pub fn focus_app(
    app_name: &str,
    desktop_entry: Option<&str>,
    state: &Rc<RefCell<NotificationState>>,
    compositor: &dyn Compositor,
) {
    if let Ok(clients) = compositor.list_clients() {
        // Try each candidate: desktop_entry first, then app_name
        let candidates: Vec<&str> = desktop_entry
            .into_iter()
            .chain(std::iter::once(app_name))
            .collect();

        for candidate in &candidates {
            let candidate_lower = candidate.to_lowercase();
            for client in &clients {
                let class_lower = client.class.to_lowercase();
                // Match: exact, class contains candidate, or candidate contains class
                if class_lower == candidate_lower
                    || class_lower.contains(&candidate_lower)
                    || candidate_lower.contains(&class_lower)
                {
                    let _ = compositor.focus_window(&client.id); // Best-effort: window may have closed
                    return;
                }
            }
        }
    }

    // App not running — try to launch it
    let class_to_find = desktop_entry.unwrap_or(app_name);
    let app_dirs = state.borrow().app_dirs.clone();
    nwg_common::launch::launch_via_compositor(
        &nwg_common::desktop::icons::get_exec(class_to_find, &app_dirs)
            .unwrap_or_else(|| class_to_find.to_string()),
        compositor,
    );
}
