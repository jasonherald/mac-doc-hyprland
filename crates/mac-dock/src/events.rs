use crate::state::DockState;
use dock_common::hyprland::events::{EventStream, HyprEvent};
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

/// Starts a background thread that listens for Hyprland events
/// and triggers UI refreshes on the main thread via polling.
/// Only rebuilds if the client list actually changed (different count
/// or different set of classes).
pub fn start_event_listener(
    state: Rc<RefCell<DockState>>,
    rebuild_fn: Rc<dyn Fn()>,
) {
    let (sender, receiver) = mpsc::channel::<String>();

    std::thread::spawn(move || {
        let mut stream = match EventStream::connect() {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to connect to Hyprland event socket: {}", e);
                return;
            }
        };

        loop {
            match stream.next_event() {
                Ok(HyprEvent::ActiveWindowV2(addr)) => {
                    if sender.send(addr).is_err() {
                        break;
                    }
                }
                Ok(_) => {} // Other events ignored
                Err(e) => {
                    log::error!("Hyprland event stream error: {}", e);
                    break;
                }
            }
        }
    });

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        let mut needs_rebuild = false;

        while let Ok(win_addr) = receiver.try_recv() {
            let last = state.borrow().last_win_addr.clone();
            if win_addr != last && !win_addr.contains(">>") {
                state.borrow_mut().last_win_addr = win_addr;
                needs_rebuild = true;
            }
        }

        if needs_rebuild {
            // Snapshot old client classes for diff
            let old_classes: Vec<String> = state
                .borrow()
                .clients
                .iter()
                .map(|c| c.class.clone())
                .collect();
            let old_active = state
                .borrow()
                .active_client
                .as_ref()
                .map(|c| c.class.clone());

            if let Err(e) = state.borrow_mut().refresh_clients() {
                log::error!("Failed to refresh clients: {}", e);
            } else {
                // Only rebuild if classes changed or active window changed
                let new_classes: Vec<String> = state
                    .borrow()
                    .clients
                    .iter()
                    .map(|c| c.class.clone())
                    .collect();
                let new_active = state
                    .borrow()
                    .active_client
                    .as_ref()
                    .map(|c| c.class.clone());

                if old_classes != new_classes || old_active != new_active {
                    rebuild_fn();
                }
            }
        }

        glib::ControlFlow::Continue
    });
}
