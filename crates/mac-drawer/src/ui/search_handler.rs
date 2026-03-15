use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::well_builder;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Connects the search entry to the well, handling search/clear/command modes.
pub fn connect_search(
    search_entry: &gtk4::SearchEntry,
    well: &gtk4::Box,
    status_label: &gtk4::Label,
    config: &Rc<DrawerConfig>,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Rc<PathBuf>,
    on_launch: &Rc<dyn Fn()>,
) {
    let config = Rc::clone(config);
    let state = Rc::clone(state);
    let well = well.clone();
    let on_launch = Rc::clone(on_launch);
    let pinned_file = Rc::clone(pinned_file);
    let status_label = status_label.clone();
    let in_search_mode = Rc::new(RefCell::new(false));

    search_entry.connect_search_changed(move |entry| {
        let phrase = entry.text().to_string();

        if phrase.is_empty() {
            if *in_search_mode.borrow() {
                *in_search_mode.borrow_mut() = false;
                well_builder::build_normal_well(
                    &well, &config, &state, &pinned_file, &on_launch,
                );
            }
            status_label.set_text("");
            return;
        }

        *in_search_mode.borrow_mut() = true;

        // Command mode (: prefix)
        if phrase.starts_with(':') {
            // Clear well for command mode
            while let Some(child) = well.first_child() {
                well.remove(&child);
            }
            if phrase.len() > 1 {
                let cmd_text = phrase.strip_prefix(':').unwrap_or(&phrase);
                status_label.set_text(&format!("Execute \"{}\"", cmd_text));
            } else {
                status_label.set_text("Execute a command");
            }
            return;
        }

        // Search mode — show matching apps + files
        well_builder::build_search_results(
            &well, &phrase, &config, &state, &pinned_file, &on_launch,
        );
        status_label.set_text("");
    });
}
