use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Builds the normal (non-search) well content as a single unified FlowBox.
///
/// Pinned items come first, followed by spacer padding to fill the row,
/// then all non-pinned apps. Everything is in one focus group so arrow
/// keys flow naturally without Tab boundaries.
pub fn build_normal_well(
    well: &gtk4::Box,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) {
    clear_well(well);

    // Create on_rebuild callback that rebuilds this well
    let well_ref = well.clone();
    let config_ref = config.clone();
    let state_ref = Rc::clone(state);
    let pinned_file_ref = pinned_file.to_path_buf();
    let on_launch_ref = Rc::clone(on_launch);
    let status_label_ref = status_label.clone();
    let on_rebuild: Rc<dyn Fn()> = Rc::new(move || {
        let well = well_ref.clone();
        let config = config_ref.clone();
        let state = Rc::clone(&state_ref);
        let pinned_file = pinned_file_ref.clone();
        let on_launch = Rc::clone(&on_launch_ref);
        let status_label = status_label_ref.clone();
        // Defer rebuild to next idle to avoid reentrancy
        gtk4::glib::idle_add_local_once(move || {
            build_normal_well(
                &well,
                &config,
                &state,
                &pinned_file,
                &on_launch,
                &status_label,
            );
        });
    });

    let flow = ui::app_grid::build_unified_flow_box(
        config,
        state,
        pinned_file,
        Rc::clone(on_launch),
        status_label,
        on_rebuild,
    );
    flow.set_halign(gtk4::Align::Center);
    well.append(&flow);
}

/// Builds search results into the well: matching apps grid + file list.
pub fn build_search_results(
    well: &gtk4::Box,
    phrase: &str,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) {
    clear_well(well);

    // App results
    let app_flow = ui::app_grid::build_app_flow_box(
        config,
        state,
        None,
        phrase,
        pinned_file,
        Rc::clone(on_launch),
        status_label,
    );
    app_flow.set_halign(gtk4::Align::Center);
    well.append(&app_flow);

    // File results (phrase > 2 chars)
    if !config.no_fs && phrase.len() > 2 {
        well.append(&divider());

        let file_results =
            ui::file_search::search_files(phrase, config, state, Rc::clone(on_launch));

        // Update status with result count
        let count = count_children(&file_results);
        if count > 0 {
            status_label.set_text(&format!(
                "{} file results | LMB: open | RMB: file manager",
                count
            ));
        }

        file_results.set_halign(gtk4::Align::Center);
        well.append(&file_results);
    }
}

fn clear_well(well: &gtk4::Box) {
    while let Some(child) = well.first_child() {
        well.remove(&child);
    }
}

fn divider() -> gtk4::Separator {
    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    sep.set_margin_top(8);
    sep.set_margin_bottom(8);
    sep.set_margin_start(16);
    sep.set_margin_end(16);
    sep
}

/// Counts direct children of a widget (for file result count display).
fn count_children(widget: &impl IsA<gtk4::Widget>) -> i32 {
    let mut count = 0;
    let mut child = widget.first_child();
    while let Some(c) = child {
        count += 1;
        child = c.next_sibling();
    }
    count
}
