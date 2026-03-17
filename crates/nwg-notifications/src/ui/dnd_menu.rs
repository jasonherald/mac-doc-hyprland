use crate::state::NotificationState;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use std::cell::RefCell;
use std::rc::Rc;

/// Timed DND options: (minutes, label).
const TIMED_OPTIONS: &[(u64, &str)] = &[
    (60, "For 1 hour"),
    (120, "For 2 hours"),
    (480, "Until tomorrow morning"),
];

/// A small popup menu for DND options, triggered by right-clicking the waybar bell.
pub struct DndMenu {
    win: gtk4::ApplicationWindow,
    backdrop: gtk4::ApplicationWindow,
    state: Rc<RefCell<NotificationState>>,
    on_state_change: Rc<dyn Fn()>,
}

impl DndMenu {
    pub fn new(
        app: &gtk4::Application,
        state: &Rc<RefCell<NotificationState>>,
        on_state_change: Rc<dyn Fn()>,
    ) -> Self {
        // Transparent backdrop for click-outside-to-close
        let backdrop = gtk4::ApplicationWindow::new(app);
        backdrop.add_css_class("dnd-menu-backdrop");
        setup_backdrop_window(&backdrop);

        let win = gtk4::ApplicationWindow::new(app);
        win.add_css_class("dnd-menu-window");
        setup_menu_window(&win);

        // Backdrop click → close menu
        let click = gtk4::GestureClick::new();
        let win_bd = win.clone();
        let backdrop_bd = backdrop.clone();
        click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            win_bd.set_visible(false);
            backdrop_bd.set_visible(false);
        });
        backdrop.add_controller(click);

        // Escape key → close menu
        let key_ctrl = gtk4::EventControllerKey::new();
        let win_esc = win.clone();
        let backdrop_esc = backdrop.clone();
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            if key == gtk4::gdk::Key::Escape {
                win_esc.set_visible(false);
                backdrop_esc.set_visible(false);
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
        win.add_controller(key_ctrl);

        backdrop.present();
        backdrop.set_visible(false);
        win.present();
        win.set_visible(false);

        Self {
            win,
            backdrop,
            state: Rc::clone(state),
            on_state_change,
        }
    }

    pub fn toggle(&self) {
        if self.win.is_visible() {
            self.win.set_visible(false);
            self.backdrop.set_visible(false);
        } else {
            self.rebuild();
            self.backdrop.set_visible(true);
            self.win.set_visible(true);
        }
    }

    fn rebuild(&self) {
        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        vbox.add_css_class("dnd-menu");
        vbox.set_margin_start(8);
        vbox.set_margin_end(8);
        vbox.set_margin_top(8);
        vbox.set_margin_bottom(8);

        // Toggle button — label reflects current state
        let is_dnd = self.state.borrow().dnd;
        let toggle_label = if is_dnd {
            "Turn off Do Not Disturb"
        } else {
            "Turn on Do Not Disturb"
        };

        let toggle_btn = gtk4::Button::with_label(toggle_label);
        toggle_btn.add_css_class("dnd-menu-item");
        toggle_btn.set_has_frame(false);
        let state_toggle = Rc::clone(&self.state);
        let on_change_toggle = Rc::clone(&self.on_state_change);
        let win_toggle = self.win.clone();
        let bd_toggle = self.backdrop.clone();
        toggle_btn.connect_clicked(move |_| {
            let new_dnd = !state_toggle.borrow().dnd;
            state_toggle.borrow_mut().dnd = new_dnd;
            state_toggle.borrow_mut().dnd_expires = None;
            log::info!("DND {}", if new_dnd { "enabled" } else { "disabled" });
            on_change_toggle();
            win_toggle.set_visible(false);
            bd_toggle.set_visible(false);
        });
        vbox.append(&toggle_btn);

        if is_dnd {
            // Show remaining time if timed DND is active
            if let Some(expiry) = self.state.borrow().dnd_expires
                && let Ok(remaining) = expiry.duration_since(std::time::SystemTime::now())
            {
                let mins = remaining.as_secs() / 60;
                let text = if mins >= 60 {
                    format!("Expires in {}h {}m", mins / 60, mins % 60)
                } else {
                    format!("Expires in {}m", mins.max(1))
                };
                let label = gtk4::Label::new(Some(&text));
                label.add_css_class("dnd-menu-expires");
                label.set_margin_top(2);
                vbox.append(&label);
            }
        } else {
            // Timed options
            let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            sep.set_margin_top(2);
            sep.set_margin_bottom(2);
            vbox.append(&sep);

            for &(minutes, label) in TIMED_OPTIONS {
                let btn = build_timed_dnd_button(
                    minutes,
                    label,
                    &self.state,
                    &self.on_state_change,
                    &self.win,
                    &self.backdrop,
                );
                vbox.append(&btn);
            }
        }

        self.win.set_child(Some(&vbox));
        self.win.set_default_size(-1, -1);
    }
}

fn setup_backdrop_window(win: &gtk4::ApplicationWindow) {
    super::window::setup_fullscreen_backdrop(win, "mac-notification-dnd-backdrop");
}

fn setup_menu_window(win: &gtk4::ApplicationWindow) {
    win.init_layer_shell();
    win.set_namespace(Some("mac-notification-dnd-menu"));
    win.set_layer(gtk4_layer_shell::Layer::Overlay);
    win.set_exclusive_zone(-1);
    win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);

    win.set_anchor(gtk4_layer_shell::Edge::Top, true);
    win.set_anchor(gtk4_layer_shell::Edge::Right, true);
    win.set_margin(gtk4_layer_shell::Edge::Top, 30);
    win.set_margin(gtk4_layer_shell::Edge::Right, 16);
}

/// Creates a button that enables DND for a fixed duration, with an auto-expire timer.
fn build_timed_dnd_button(
    minutes: u64,
    label: &str,
    state: &Rc<RefCell<NotificationState>>,
    on_state_change: &Rc<dyn Fn()>,
    win: &gtk4::ApplicationWindow,
    backdrop: &gtk4::ApplicationWindow,
) -> gtk4::Button {
    let btn = gtk4::Button::with_label(label);
    btn.add_css_class("dnd-menu-item");
    btn.set_has_frame(false);

    let state_btn = Rc::clone(state);
    let on_change = Rc::clone(on_state_change);
    let win_btn = win.clone();
    let bd_btn = backdrop.clone();
    btn.connect_clicked(move |_| {
        state_btn.borrow_mut().dnd = true;
        let expiry = std::time::SystemTime::now() + std::time::Duration::from_secs(minutes * 60);
        state_btn.borrow_mut().dnd_expires = Some(expiry);
        log::info!("DND enabled for {} minutes", minutes);

        let state_timer = Rc::clone(&state_btn);
        let on_change_timer = Rc::clone(&on_change);
        gtk4::glib::timeout_add_local_once(
            std::time::Duration::from_secs(minutes * 60),
            move || {
                if state_timer.borrow().dnd_expires.is_some() {
                    state_timer.borrow_mut().dnd = false;
                    state_timer.borrow_mut().dnd_expires = None;
                    log::info!("Timed DND expired");
                    on_change_timer();
                }
            },
        );

        on_change();
        win_btn.set_visible(false);
        bd_btn.set_visible(false);
    });

    btn
}
