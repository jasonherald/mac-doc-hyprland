use crate::state::DockState;
use gtk4::glib;
use nwg_dock_common::compositor::{Compositor, WmEvent};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

/// Checks for new events and triggers a rebuild if the client list changed.
/// Skips rebuild while a drag is in progress to avoid destroying widgets mid-drag.
/// Events during drag are still drained (so they don't queue up), and the
/// drag-end handler triggers its own deferred rebuild which picks up any changes.
fn poll_and_rebuild(
    receiver: &mpsc::Receiver<String>,
    state: &Rc<RefCell<DockState>>,
    rebuild_fn: &Rc<dyn Fn()>,
) {
    if drain_new_events(receiver, state)
        && needs_rebuild(state)
        && state.borrow().drag_source_index.is_none()
    {
        rebuild_fn();
    }
}

/// Drains pending window-change events and returns true if a new relevant event was seen.
fn drain_new_events(receiver: &mpsc::Receiver<String>, state: &Rc<RefCell<DockState>>) -> bool {
    let mut changed = false;
    while let Ok(win_addr) = receiver.try_recv() {
        let last = state.borrow().last_win_addr.clone();
        // Filter out Hyprland layer/redirect events that aren't real window addresses
        if win_addr != last && !win_addr.contains(">>") {
            state.borrow_mut().last_win_addr = win_addr;
            changed = true;
        }
    }
    changed
}

/// Snapshots old client state, refreshes from compositor, and returns
/// whether the client list or active window changed (requiring a rebuild).
fn needs_rebuild(state: &Rc<RefCell<DockState>>) -> bool {
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
        return false;
    }

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

    old_classes != new_classes || old_active != new_active
}

/// Starts a background thread that listens for compositor events
/// and triggers UI refreshes on the main thread via polling.
/// Only rebuilds if the client list actually changed (different count
/// or different set of classes).
pub fn start_event_listener(
    state: Rc<RefCell<DockState>>,
    rebuild_fn: Rc<dyn Fn()>,
    compositor: Rc<dyn Compositor>,
) {
    let (sender, receiver) = mpsc::channel::<String>();

    // Create the event stream on the main thread, then move it to the background
    let mut stream = match compositor.event_stream() {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to connect to compositor event stream: {}", e);
            return;
        }
    };

    std::thread::spawn(move || {
        loop {
            match stream.next_event() {
                Ok(WmEvent::ActiveWindowChanged(id)) => {
                    if sender.send(id).is_err() {
                        break;
                    }
                }
                Ok(_) => {} // Other events ignored
                Err(e) => {
                    log::error!("Compositor event stream error: {}", e);
                    break;
                }
            }
        }
    });

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        poll_and_rebuild(&receiver, &state, &rebuild_fn);
        glib::ControlFlow::Continue
    });
}
