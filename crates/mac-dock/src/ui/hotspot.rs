use crate::config::DockConfig;
use crate::state::DockState;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates a hotspot window for auto-hide mouse detection.
///
/// The hotspot has two regions:
/// - A detector box that records when the mouse enters
/// - A hotspot box that triggers the dock to show if the mouse moves fast enough
pub fn setup_hotspot(
    monitor: &gtk4::gdk::Monitor,
    dock_window: &gtk4::ApplicationWindow,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    app: &gtk4::Application,
) -> gtk4::ApplicationWindow {
    let hotspot_win = gtk4::ApplicationWindow::new(app);
    hotspot_win.init_layer_shell();
    hotspot_win.set_namespace(Some("hotspot"));
    hotspot_win.set_monitor(Some(monitor));

    let orientation = if config.position == "bottom" || config.position == "top" {
        gtk4::Orientation::Vertical
    } else {
        gtk4::Orientation::Horizontal
    };
    let bx = gtk4::Box::new(orientation, 0);
    hotspot_win.set_child(Some(&bx));

    // Detector box — records entry time
    let detector = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    detector.set_widget_name("detector-box");

    // Hotspot box — triggers dock show
    let hotspot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    hotspot.set_widget_name("hotspot-box");

    // Get dock window size for sizing the hotspot regions
    let (dock_w, dock_h) = (dock_window.width(), dock_window.height());

    match config.position.as_str() {
        "bottom" | "top" => {
            detector.set_size_request(dock_w.max(100), dock_h.max(20) / 3);
            hotspot.set_size_request(dock_w.max(100), 2);

            if config.position == "bottom" {
                bx.append(&detector);
                bx.append(&hotspot);
                hotspot_win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
            } else {
                bx.append(&hotspot);
                bx.append(&detector);
                hotspot_win.set_anchor(gtk4_layer_shell::Edge::Top, true);
            }
            hotspot_win.set_anchor(gtk4_layer_shell::Edge::Left, config.full);
            hotspot_win.set_anchor(gtk4_layer_shell::Edge::Right, config.full);
        }
        "left" | "right" => {
            detector.set_size_request(dock_w.max(20) / 3, dock_h.max(100));
            hotspot.set_size_request(2, dock_h.max(100));

            if config.position == "left" {
                bx.append(&hotspot);
                bx.append(&detector);
                hotspot_win.set_anchor(gtk4_layer_shell::Edge::Left, true);
            } else {
                bx.append(&detector);
                bx.append(&hotspot);
                hotspot_win.set_anchor(gtk4_layer_shell::Edge::Right, true);
            }
            hotspot_win.set_anchor(gtk4_layer_shell::Edge::Top, config.full);
            hotspot_win.set_anchor(gtk4_layer_shell::Edge::Bottom, config.full);
        }
        _ => {}
    }

    // Layer
    if config.hotspot_layer == "top" {
        hotspot_win.set_layer(gtk4_layer_shell::Layer::Top);
    } else {
        hotspot_win.set_layer(gtk4_layer_shell::Layer::Overlay);
    }
    hotspot_win.set_exclusive_zone(-1);

    // Detector enter → record timestamp
    let state_detector = Rc::clone(state);
    let detector_motion = gtk4::EventControllerMotion::new();
    detector_motion.connect_enter(move |_, _, _| {
        let now = now_millis();
        state_detector.borrow_mut().detector_entered_at = now;
    });
    detector.add_controller(detector_motion);

    // Hotspot enter → show dock if fast enough
    let dock_ref = dock_window.clone();
    let state_hotspot = Rc::clone(state);
    let delay = config.hotspot_delay;
    let hotspot_motion = gtk4::EventControllerMotion::new();
    hotspot_motion.connect_enter(move |_, _, _| {
        let now = now_millis();
        let entered_at = state_hotspot.borrow().detector_entered_at;
        let elapsed = now - entered_at;

        if elapsed <= delay || delay == 0 {
            log::debug!("Delay {} <= {} ms, showing dock", elapsed, delay);
            dock_ref.set_visible(false);
            dock_ref.set_visible(true);
        } else {
            log::debug!("Delay {} > {} ms, not showing dock", elapsed, delay);
        }
    });
    hotspot.add_controller(hotspot_motion);

    // Leave hotspot → hide dock after timeout
    if config.autohide {
        let dock_hide = dock_window.clone();
        let state_leave = Rc::clone(state);
        let leave_motion = gtk4::EventControllerMotion::new();
        leave_motion.connect_leave(move |_| {
            state_leave.borrow_mut().mouse_inside_hotspot = false;
            let dock_ref = dock_hide.clone();
            let state_ref = Rc::clone(&state_leave);
            gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_millis(1000),
                move || {
                    let s = state_ref.borrow();
                    if !s.mouse_inside_dock && !s.mouse_inside_hotspot {
                        dock_ref.set_visible(false);
                    }
                },
            );
        });
        hotspot_win.add_controller(leave_motion);

        let state_enter = Rc::clone(state);
        let enter_motion = gtk4::EventControllerMotion::new();
        enter_motion.connect_enter(move |_, _, _| {
            state_enter.borrow_mut().mouse_inside_hotspot = true;
        });
        hotspot_win.add_controller(enter_motion);
    }

    hotspot_win
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
