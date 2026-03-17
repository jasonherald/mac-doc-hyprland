use crate::state::DockState;
use gtk4::gdk;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Maps compositor output names to GDK monitors.
///
/// Uses the compositor monitor list (by index) to match against GDK monitors.
pub fn map_outputs(state: &Rc<RefCell<DockState>>) -> HashMap<String, gdk::Monitor> {
    let mut result = HashMap::new();

    if let Err(e) = state.borrow_mut().refresh_monitors() {
        log::error!("Error listing monitors: {}", e);
        return result;
    }

    let display = match gdk::Display::default() {
        Some(d) => d,
        None => {
            log::error!("No default GDK display");
            return result;
        }
    };

    let monitors = display.monitors();
    let wm_monitors = state.borrow().monitors.clone();

    for (i, wm_mon) in wm_monitors.iter().enumerate() {
        if let Some(item) = monitors.item(i as u32)
            && let Ok(mon) = item.downcast::<gdk::Monitor>()
        {
            result.insert(wm_mon.name.clone(), mon);
        }
    }

    result
}

/// Resolves which monitors to show the dock on, based on the -o flag.
pub fn resolve_monitors(
    state: &Rc<RefCell<DockState>>,
    config: &crate::config::DockConfig,
) -> Vec<gdk::Monitor> {
    let output_map = map_outputs(state);
    if !config.output.is_empty() {
        if let Some(mon) = output_map.get(&config.output) {
            vec![mon.clone()]
        } else {
            log::warn!(
                "Target output '{}' not found, using all monitors",
                config.output
            );
            list_gdk_monitors()
        }
    } else {
        list_gdk_monitors()
    }
}

/// Lists all GDK monitors.
pub fn list_gdk_monitors() -> Vec<gdk::Monitor> {
    let mut monitors = Vec::new();
    let display = match gdk::Display::default() {
        Some(d) => d,
        None => return monitors,
    };

    let model = display.monitors();
    for i in 0..model.n_items() {
        if let Some(item) = model.item(i)
            && let Ok(mon) = item.downcast::<gdk::Monitor>()
        {
            monitors.push(mon);
        }
    }
    monitors
}
