use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::well_builder;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Connects the search entry to the well, handling search/clear/command modes.
#[allow(clippy::too_many_arguments)]
pub fn connect_search(
    search_entry: &gtk4::SearchEntry,
    well: &gtk4::Box,
    pinned_box: &gtk4::Box,
    status_label: &gtk4::Label,
    config: &Rc<DrawerConfig>,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Rc<PathBuf>,
    on_launch: &Rc<dyn Fn()>,
) {
    let config = Rc::clone(config);
    let state = Rc::clone(state);
    let well = well.clone();
    let pinned_box = pinned_box.clone();
    let on_launch = Rc::clone(on_launch);
    let pinned_file = Rc::clone(pinned_file);
    let status_label = status_label.clone();
    let in_search_mode = Rc::new(RefCell::new(false));

    search_entry.connect_search_changed(move |entry| {
        let phrase = entry.text().to_string();

        if phrase.is_empty() {
            if *in_search_mode.borrow() {
                *in_search_mode.borrow_mut() = false;
                state.borrow_mut().active_search.clear();
                well_builder::restore_normal_well(
                    &well,
                    &pinned_box,
                    &config,
                    &state,
                    &pinned_file,
                    &on_launch,
                    &status_label,
                );
            }
            status_label.set_text("");
            return;
        }

        *in_search_mode.borrow_mut() = true;

        // Command mode (: prefix)
        if phrase.starts_with(':') {
            while let Some(child) = well.first_child() {
                well.remove(&child);
            }
            pinned_box.set_visible(false);
            if phrase.len() > 1 {
                let cmd_text = phrase.strip_prefix(':').unwrap_or(&phrase);
                status_label.set_text(&format!("Execute \"{}\"", cmd_text));
            } else {
                status_label.set_text("Execute a command");
            }
            return;
        }

        // Search mode — track in state and show matching apps + files
        state.borrow_mut().active_search = phrase.clone();
        well_builder::build_search_results(
            &well,
            &pinned_box,
            &phrase,
            &config,
            &state,
            &pinned_file,
            &on_launch,
            &status_label,
        );
    });
}
