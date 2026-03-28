use gtk4::gdk;
use gtk4::prelude::*;
use std::collections::HashMap;

/// Maps output connector names to GDK monitors.
///
/// Uses `gdk::Monitor::connector()` for stable name-based mapping
/// instead of index-based mapping (which drifts on monitor hotplug).
pub fn map_outputs_by_connector() -> HashMap<String, gdk::Monitor> {
    let mut result = HashMap::new();
    let Some(display) = gdk::Display::default() else {
        log::error!("No default GDK display");
        return result;
    };

    let model = display.monitors();
    for i in 0..model.n_items() {
        if let Some(item) = model.item(i)
            && let Ok(mon) = item.downcast::<gdk::Monitor>()
            && let Some(name) = mon.connector()
        {
            result.insert(name.to_string(), mon);
        }
    }
    result
}

/// Resolves which monitors to show the dock on, based on the -o flag.
/// Returns (output_name, gdk_monitor) pairs.
pub fn resolve_monitors(config: &crate::config::DockConfig) -> Vec<(String, gdk::Monitor)> {
    let output_map = map_outputs_by_connector();
    if !config.output.is_empty() {
        if let Some(mon) = output_map.get(&config.output) {
            vec![(config.output.clone(), mon.clone())]
        } else {
            log::warn!(
                "Target output '{}' not found, using all monitors",
                config.output
            );
            output_map.into_iter().collect()
        }
    } else {
        output_map.into_iter().collect()
    }
}
