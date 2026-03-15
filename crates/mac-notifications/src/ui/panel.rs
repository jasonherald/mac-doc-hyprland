use super::constants::*;
use super::panel_content;
use crate::state::NotificationState;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use std::cell::RefCell;
use std::rc::Rc;

/// The slide-out notification history panel.
pub struct NotificationPanel {
    pub win: gtk4::ApplicationWindow,
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

        // Header
        let header = build_header(state, &on_state_change);
        panel_box.append(&header);

        // Scrolled list
        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);
        panel_box.append(&scrolled);

        let list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        list_box.add_css_class("panel-list");
        scrolled.set_child(Some(&list_box));

        let panel = Self {
            win,
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

        panel
    }

    /// Toggles panel visibility with slide animation.
    pub fn toggle(&self) {
        if self.revealer.reveals_child() {
            // Slide out, then hide window after animation completes
            self.revealer.set_reveal_child(false);
            let win = self.win.clone();
            gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_millis(PANEL_REVEAL_DURATION_MS as u64),
                move || {
                    win.set_visible(false);
                },
            );
        } else {
            // Rebuild, show window, then slide in
            let list = self.list_box.clone();
            let state = Rc::clone(&self.state);
            let on_click = Rc::clone(&self.on_notification_click);
            let on_change = Rc::clone(&self.on_state_change);
            let win = self.win.clone();
            let revealer = self.revealer.clone();
            gtk4::glib::idle_add_local_once(move || {
                rebuild_list(&list, &state, on_click, on_change);
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

fn build_header(
    state: &Rc<RefCell<NotificationState>>,
    on_state_change: &Rc<dyn Fn()>,
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
    clear_btn.connect_clicked(move |_| {
        state_clear.borrow_mut().dismiss_all();
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
