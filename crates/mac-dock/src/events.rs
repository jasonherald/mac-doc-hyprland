use crate::state::DockState;
use dock_common::hyprland::events::{EventStream, HyprEvent};
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

/// Starts a background thread that listens for Hyprland events
/// and triggers UI refreshes on the main thread via polling.
pub fn start_event_listener(
    state: Rc<RefCell<DockState>>,
    rebuild_fn: Rc<dyn Fn()>,
) {
    let (sender, receiver) = mpsc::channel::<String>();

    // Background thread reads from Hyprland socket2
    std::thread::spawn(move || {
        let mut stream = match EventStream::connect() {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to connect to Hyprland event socket: {}", e);
                return;
            }
        };

        while let Some(event) = stream.next_event() {
            if let HyprEvent::ActiveWindowV2(addr) = event
                && sender.send(addr).is_err() {
                    break;
                }
        }
    });

    // Poll for events on the main thread
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        while let Ok(win_addr) = receiver.try_recv() {
            let last = state.borrow().last_win_addr.clone();
            if win_addr != last && !win_addr.contains(">>") {
                state.borrow_mut().last_win_addr = win_addr;
                if let Err(e) = state.borrow_mut().refresh_clients() {
                    log::error!("Failed to refresh clients: {}", e);
                } else {
                    rebuild_fn();
                }
            }
        }
        glib::ControlFlow::Continue
    });
}
