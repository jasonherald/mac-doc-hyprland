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
}

impl NotificationPanel {
    /// Creates the panel window (starts hidden).
    pub fn new(
        app: &gtk4::Application,
        state: &Rc<RefCell<NotificationState>>,
        on_notification_click: Rc<dyn Fn(u32)>,
    ) -> Self {
        let win = gtk4::ApplicationWindow::new(app);
        win.add_css_class("notification-panel-window");
        win.set_width_request(PANEL_WIDTH);
        setup_panel_window(&win);

        // Panel content container
        let panel_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        panel_box.add_css_class("notification-panel");
        panel_box.set_width_request(PANEL_WIDTH);
        win.set_child(Some(&panel_box));

        // Header
        let header = build_header(state);
        panel_box.append(&header);

        // Scrolled list
        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);
        panel_box.append(&scrolled);

        let list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        list_box.add_css_class("panel-list");
        scrolled.set_child(Some(&list_box));

        // Dummy revealer field (not used, kept for struct compat)
        let revealer = gtk4::Revealer::new();

        let panel = Self {
            win,
            revealer,
            list_box,
            state: Rc::clone(state),
            on_notification_click,
        };

        panel.rebuild();
        // Present once at startup then immediately hide — establishes the layer surface
        panel.win.present();
        panel.win.set_visible(false);

        panel
    }

    /// Toggles panel visibility.
    pub fn toggle(&self) {
        if self.win.is_visible() {
            self.win.set_visible(false);
        } else {
            // Defer rebuild+show to next idle to avoid reentrancy
            let list = self.list_box.clone();
            let state = Rc::clone(&self.state);
            let on_click = Rc::clone(&self.on_notification_click);
            let win = self.win.clone();
            gtk4::glib::idle_add_local_once(move || {
                rebuild_list(&list, &state, on_click);
                win.set_visible(true);
            });
        }
    }

    /// Returns whether the panel is currently visible.
    pub fn is_visible(&self) -> bool {
        self.win.is_visible()
    }

    /// Rebuilds the notification list content.
    pub fn rebuild(&self) {
        rebuild_list(
            &self.list_box,
            &self.state,
            Rc::clone(&self.on_notification_click),
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

fn build_header(state: &Rc<RefCell<NotificationState>>) -> gtk4::Box {
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
    });
    header.append(&dnd_btn);

    // Clear all
    let clear_btn = gtk4::Button::with_label("Clear All");
    clear_btn.add_css_class("panel-clear");
    let state_clear = Rc::clone(state);
    clear_btn.connect_clicked(move |_| {
        state_clear.borrow_mut().dismiss_all();
        log::info!("Cleared all notifications");
    });
    header.append(&clear_btn);

    header
}

/// Rebuilds the notification list in the panel.
fn rebuild_list(
    list_box: &gtk4::Box,
    state: &Rc<RefCell<NotificationState>>,
    on_click: Rc<dyn Fn(u32)>,
) {
    let state_ref = Rc::clone(state);
    let list_ref = list_box.clone();
    let on_rebuild: Rc<dyn Fn()> = Rc::new(move || {
        // One level deep — don't recurse further
    });

    panel_content::build_grouped_list(list_box, state, on_click, on_rebuild);
}
