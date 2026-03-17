use super::constants::{PANEL_REVEAL_DURATION_MS, PANEL_WIDTH};
use super::panel_content;
use crate::state::NotificationState;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use std::cell::RefCell;
use std::rc::Rc;

/// The slide-out notification history panel.
pub struct NotificationPanel {
    pub win: gtk4::ApplicationWindow,
    backdrop: gtk4::ApplicationWindow,
    revealer: gtk4::Revealer,
    list_box: gtk4::Box,
    state: Rc<RefCell<NotificationState>>,
    on_notification_click: Rc<dyn Fn(u32)>,
    on_state_change: Rc<dyn Fn()>,
}

impl NotificationPanel {
    /// Creates the panel window (starts hidden).
    pub fn new(
        app: &gtk4::Application,
        state: &Rc<RefCell<NotificationState>>,
        on_notification_click: Rc<dyn Fn(u32)>,
        on_state_change: Rc<dyn Fn()>,
    ) -> Self {
        // Transparent backdrop — catches clicks outside the panel
        let backdrop = gtk4::ApplicationWindow::new(app);
        backdrop.add_css_class("notification-backdrop");
        setup_backdrop_window(&backdrop);

        // Panel window
        let win = gtk4::ApplicationWindow::new(app);
        win.add_css_class("notification-panel-window");
        win.set_width_request(PANEL_WIDTH);
        setup_panel_window(&win);

        // Revealer for slide animation
        let revealer = gtk4::Revealer::new();
        revealer.set_transition_type(gtk4::RevealerTransitionType::SlideLeft);
        revealer.set_transition_duration(PANEL_REVEAL_DURATION_MS);
        revealer.set_reveal_child(false);
        win.set_child(Some(&revealer));

        // Panel content container (inside revealer)
        let panel_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        panel_box.add_css_class("notification-panel");
        panel_box.set_width_request(PANEL_WIDTH);
        revealer.set_child(Some(&panel_box));

        // Scrolled list (created before header so Clear All can reference it)
        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);

        let list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        list_box.add_css_class("panel-list");
        scrolled.set_child(Some(&list_box));

        // Header (needs list_box ref for Clear All)
        let header = build_header(state, &on_state_change, &list_box);
        panel_box.append(&header);
        panel_box.append(&scrolled);

        // Backdrop click → close panel
        let backdrop_click = gtk4::GestureClick::new();
        let revealer_bd = revealer.clone();
        let win_bd = win.clone();
        let backdrop_bd = backdrop.clone();
        backdrop_click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            hide_panel(&revealer_bd, &win_bd, &backdrop_bd);
        });
        backdrop.add_controller(backdrop_click);

        // Escape key → close panel
        let key_ctrl = gtk4::EventControllerKey::new();
        let revealer_esc = revealer.clone();
        let win_esc = win.clone();
        let backdrop_esc = backdrop.clone();
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            if key == gtk4::gdk::Key::Escape {
                hide_panel(&revealer_esc, &win_esc, &backdrop_esc);
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
        win.add_controller(key_ctrl);

        let panel = Self {
            win,
            backdrop,
            revealer,
            list_box,
            state: Rc::clone(state),
            on_notification_click,
            on_state_change,
        };

        panel.rebuild();
        // Present once at startup then immediately hide — establishes the layer surface
        panel.win.present();
        panel.win.set_visible(false);
        panel.backdrop.present();
        panel.backdrop.set_visible(false);

        panel
    }

    /// Toggles panel visibility with slide animation.
    pub fn toggle(&self) {
        if self.revealer.reveals_child() {
            hide_panel(&self.revealer, &self.win, &self.backdrop);
        } else {
            // Rebuild, show backdrop + window, then slide in
            let list = self.list_box.clone();
            let state = Rc::clone(&self.state);
            let on_click = Rc::clone(&self.on_notification_click);
            let on_change = Rc::clone(&self.on_state_change);
            let win = self.win.clone();
            let backdrop = self.backdrop.clone();
            let revealer = self.revealer.clone();
            gtk4::glib::idle_add_local_once(move || {
                rebuild_list(&list, &state, on_click, on_change);
                backdrop.set_visible(true);
                win.set_visible(true);
                revealer.set_reveal_child(true);
            });
        }
    }

    /// Returns whether the panel is currently visible.
    pub fn is_visible(&self) -> bool {
        self.revealer.reveals_child()
    }

    /// Rebuilds the notification list content.
    pub fn rebuild(&self) {
        rebuild_list(
            &self.list_box,
            &self.state,
            Rc::clone(&self.on_notification_click),
            Rc::clone(&self.on_state_change),
        );
    }
}

/// Hides the panel with slide animation and removes the backdrop.
fn hide_panel(
    revealer: &gtk4::Revealer,
    win: &gtk4::ApplicationWindow,
    backdrop: &gtk4::ApplicationWindow,
) {
    revealer.set_reveal_child(false);
    backdrop.set_visible(false);
    let win = win.clone();
    gtk4::glib::timeout_add_local_once(
        std::time::Duration::from_millis(PANEL_REVEAL_DURATION_MS as u64),
        move || {
            win.set_visible(false);
        },
    );
}

fn setup_panel_window(win: &gtk4::ApplicationWindow) {
    win.init_layer_shell();
    win.set_namespace(Some("mac-notification-panel"));
    win.set_layer(gtk4_layer_shell::Layer::Overlay);
    win.set_exclusive_zone(-1);
    win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);

    // Anchor to right edge, full height
    win.set_anchor(gtk4_layer_shell::Edge::Top, true);
    win.set_anchor(gtk4_layer_shell::Edge::Right, true);
    win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
}

fn setup_backdrop_window(win: &gtk4::ApplicationWindow) {
    win.init_layer_shell();
    win.set_namespace(Some("mac-notification-backdrop"));
    win.set_layer(gtk4_layer_shell::Layer::Overlay);
    win.set_exclusive_zone(-1);
    win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);

    // Full screen — anchor all edges
    win.set_anchor(gtk4_layer_shell::Edge::Top, true);
    win.set_anchor(gtk4_layer_shell::Edge::Right, true);
    win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
    win.set_anchor(gtk4_layer_shell::Edge::Left, true);
}

fn build_header(
    state: &Rc<RefCell<NotificationState>>,
    on_state_change: &Rc<dyn Fn()>,
    list_box: &gtk4::Box,
) -> gtk4::Box {
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    header.add_css_class("panel-header");
    header.set_margin_start(12);
    header.set_margin_end(12);
    header.set_margin_top(12);
    header.set_margin_bottom(8);

    let title = gtk4::Label::new(Some("Notifications"));
    title.add_css_class("panel-title");
    title.set_hexpand(true);
    title.set_halign(gtk4::Align::Start);
    header.append(&title);

    // DND toggle
    let dnd_btn = gtk4::Button::from_icon_name("notifications-disabled-symbolic");
    dnd_btn.add_css_class("panel-dnd");
    dnd_btn.set_tooltip_text(Some("Do Not Disturb"));
    let state_dnd = Rc::clone(state);
    let on_change_dnd = Rc::clone(on_state_change);
    dnd_btn.connect_clicked(move |btn| {
        let new_dnd = !state_dnd.borrow().dnd;
        state_dnd.borrow_mut().dnd = new_dnd;
        let icon = if new_dnd {
            "notifications-disabled-symbolic"
        } else {
            "preferences-system-notifications-symbolic"
        };
        btn.set_icon_name(icon);
        log::info!("DND {}", if new_dnd { "enabled" } else { "disabled" });
        on_change_dnd();
    });
    header.append(&dnd_btn);

    // Clear all
    let clear_btn = gtk4::Button::with_label("Clear All");
    clear_btn.add_css_class("panel-clear");
    let state_clear = Rc::clone(state);
    let on_change_clear = Rc::clone(on_state_change);
    let list_clear = list_box.clone();
    clear_btn.connect_clicked(move |_| {
        state_clear.borrow_mut().dismiss_all();
        // Rebuild list to show empty state
        while let Some(child) = list_clear.first_child() {
            list_clear.remove(&child);
        }
        let empty = gtk4::Label::new(Some("No notifications"));
        empty.add_css_class("panel-empty");
        empty.set_margin_top(40);
        list_clear.append(&empty);
        log::info!("Cleared all notifications");
        on_change_clear();
    });
    header.append(&clear_btn);

    header
}

/// Rebuilds the notification list in the panel.
fn rebuild_list(
    list_box: &gtk4::Box,
    state: &Rc<RefCell<NotificationState>>,
    on_click: Rc<dyn Fn(u32)>,
    on_state_change: Rc<dyn Fn()>,
) {
    // Build the on_rebuild callback that re-invokes this function on next idle.
    // Deferred via idle_add to avoid reentrancy during button click handlers.
    let list_rebuild = list_box.clone();
    let state_rebuild = Rc::clone(state);
    let on_click_rebuild = Rc::clone(&on_click);
    let on_change_rebuild = Rc::clone(&on_state_change);
    let on_rebuild: Rc<dyn Fn()> = Rc::new(move || {
        let list = list_rebuild.clone();
        let state = Rc::clone(&state_rebuild);
        let on_click = Rc::clone(&on_click_rebuild);
        let on_change = Rc::clone(&on_change_rebuild);
        gtk4::glib::idle_add_local_once(move || {
            rebuild_list(&list, &state, on_click, Rc::clone(&on_change));
            on_change();
        });
    });

    panel_content::build_grouped_list(list_box, state, on_click, on_rebuild);
}
