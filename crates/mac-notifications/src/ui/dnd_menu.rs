use crate::state::NotificationState;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use std::cell::RefCell;
use std::rc::Rc;

/// DND duration options in minutes (0 = permanent toggle).
const DND_DURATIONS: &[(u64, &str)] = &[
    (0, "Do Not Disturb"),
    (60, "For 1 hour"),
    (120, "For 2 hours"),
    (480, "Until tomorrow morning"),
];

/// A small popup menu for DND options, triggered by right-clicking the waybar bell.
pub struct DndMenu {
    win: gtk4::ApplicationWindow,
}

impl DndMenu {
    pub fn new(
        app: &gtk4::Application,
        state: &Rc<RefCell<NotificationState>>,
        on_state_change: Rc<dyn Fn()>,
    ) -> Self {
        let win = gtk4::ApplicationWindow::new(app);
        win.add_css_class("dnd-menu-window");
        setup_menu_window(&win);

        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        vbox.add_css_class("dnd-menu");
        vbox.set_margin_start(8);
        vbox.set_margin_end(8);
        vbox.set_margin_top(8);
        vbox.set_margin_bottom(8);

        for &(minutes, label) in DND_DURATIONS {
            let btn = gtk4::Button::with_label(label);
            btn.add_css_class("dnd-menu-item");
            btn.set_has_frame(false);

            let state_btn = Rc::clone(state);
            let on_change = Rc::clone(&on_state_change);
            let win_btn = win.clone();
            btn.connect_clicked(move |_| {
                if minutes == 0 {
                    // Toggle permanent DND
                    let new_dnd = !state_btn.borrow().dnd;
                    state_btn.borrow_mut().dnd = new_dnd;
                    state_btn.borrow_mut().dnd_expires = None;
                    log::info!("DND {}", if new_dnd { "enabled" } else { "disabled" });
                } else {
                    // Timed DND
                    state_btn.borrow_mut().dnd = true;
                    let expiry =
                        std::time::SystemTime::now() + std::time::Duration::from_secs(minutes * 60);
                    state_btn.borrow_mut().dnd_expires = Some(expiry);
                    log::info!("DND enabled for {} minutes", minutes);

                    // Schedule auto-disable
                    let state_timer = Rc::clone(&state_btn);
                    let on_change_timer = Rc::clone(&on_change);
                    gtk4::glib::timeout_add_local_once(
                        std::time::Duration::from_secs(minutes * 60),
                        move || {
                            // Only disable if the expiry hasn't been changed
                            if state_timer.borrow().dnd_expires.is_some() {
                                state_timer.borrow_mut().dnd = false;
                                state_timer.borrow_mut().dnd_expires = None;
                                log::info!("Timed DND expired");
                                on_change_timer();
                            }
                        },
                    );
                }
                on_change();
                win_btn.set_visible(false);
            });
            vbox.append(&btn);
        }

        win.set_child(Some(&vbox));

        // Click outside to close
        let backdrop_gesture = gtk4::GestureClick::new();
        let win_close = win.clone();
        backdrop_gesture.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            win_close.set_visible(false);
        });

        win.present();
        win.set_visible(false);

        Self { win }
    }

    pub fn toggle(&self) {
        if self.win.is_visible() {
            self.win.set_visible(false);
        } else {
            // Update the toggle label to reflect current state
            self.win.set_visible(true);
        }
    }
}

fn setup_menu_window(win: &gtk4::ApplicationWindow) {
    win.init_layer_shell();
    win.set_namespace(Some("mac-notification-dnd-menu"));
    win.set_layer(gtk4_layer_shell::Layer::Overlay);
    win.set_exclusive_zone(-1);
    win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);

    // Position: top-right, below waybar
    win.set_anchor(gtk4_layer_shell::Edge::Top, true);
    win.set_anchor(gtk4_layer_shell::Edge::Right, true);
    win.set_margin(gtk4_layer_shell::Edge::Top, 30);
    win.set_margin(gtk4_layer_shell::Edge::Right, 16);
}
