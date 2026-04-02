use crate::state::DockState;
use crate::ui::constants::LAUNCH_ANIMATION_TIMEOUT_SECS;
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;

/// Starts a launch bounce animation for the given app ID.
/// Inserts the ID into `state.launching`, cancels any existing timeout
/// for the same app, registers a new timeout, and triggers a rebuild
/// to apply the CSS class immediately.
pub fn start(app_id: &str, state: &Rc<RefCell<DockState>>, rebuild: &Rc<dyn Fn()>) {
    let id = app_id.to_lowercase();
    {
        let mut s = state.borrow_mut();
        s.launching.insert(id.clone());
        // Cancel previous timeout for this app (double-click resets the timer)
        if let Some(old) = s.launch_timeouts.remove(&id) {
            old.remove();
        }
    }

    let state_ref = Rc::clone(state);
    let rebuild_ref = Rc::clone(rebuild);
    let id_timeout = id.clone();
    let source_id = glib::timeout_add_local_once(
        std::time::Duration::from_secs(LAUNCH_ANIMATION_TIMEOUT_SECS),
        move || {
            let mut s = state_ref.borrow_mut();
            if s.launching.remove(&id_timeout) {
                s.launch_timeouts.remove(&id_timeout);
                drop(s);
                rebuild_ref();
            }
        },
    );
    state.borrow_mut().launch_timeouts.insert(id, source_id);

    // Rebuild immediately to show the animation
    rebuild();
}

/// Cancels launch animations for apps that now have visible windows.
/// Called from the event poller after detecting new clients.
pub fn cancel_matched(state: &Rc<RefCell<DockState>>) -> bool {
    let mut s = state.borrow_mut();
    if s.launching.is_empty() {
        return false;
    }

    let current_classes: Vec<String> = s.clients.iter().map(|c| c.class.to_lowercase()).collect();

    let launching_snapshot: Vec<String> = s.launching.iter().cloned().collect();
    let mut cancelled = false;
    for app_id in launching_snapshot {
        // Match by exact class, hyphen↔space variant, or WMClass mapping
        let alt_id = crate::state::hyphen_space_variant(&app_id);
        let matched = current_classes.contains(&app_id)
            || current_classes.contains(&alt_id)
            || s.wm_class_to_desktop_id
                .iter()
                .any(|(wm_class, desktop_id)| {
                    desktop_id.eq_ignore_ascii_case(&app_id)
                        && current_classes.contains(&wm_class.to_lowercase())
                });
        if matched {
            s.launching.remove(&app_id);
            if let Some(source_id) = s.launch_timeouts.remove(&app_id) {
                source_id.remove();
            }
            cancelled = true;
        }
    }
    cancelled
}
