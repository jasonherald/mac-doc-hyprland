use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Builds the normal (non-search) well content: favorites → divider → all apps.
pub fn build_normal_well(
    well: &gtk4::Box,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) {
    clear_well(well);

    // Favorites section (if any pinned)
    let pinned = state.borrow().pinned.clone();
    if !pinned.is_empty() {
        well.append(&section_header("Favorites"));

        let pinned_flow = ui::pinned::build_pinned_flow_box(
            config,
            state,
            pinned_file,
            Rc::clone(on_launch),
            status_label,
        );
        pinned_flow.set_hexpand(true);
        well.append(&pinned_flow);
        well.append(&divider());
    }

    // All apps
    well.append(&section_header("Applications"));

    let flow = ui::app_grid::build_app_flow_box(
        config,
        state,
        None,
        "",
        pinned_file,
        Rc::clone(on_launch),
        status_label,
    );
    flow.set_hexpand(true);
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

    well.append(&section_header("Search Results"));

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
    app_flow.set_hexpand(true);
    well.append(&app_flow);

    // File results (phrase > 2 chars)
    if !config.no_fs && phrase.len() > 2 {
        well.append(&divider());
        well.append(&section_header("Files"));

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

        well.append(&file_results);
    }
}

fn clear_well(well: &gtk4::Box) {
    while let Some(child) = well.first_child() {
        well.remove(&child);
    }
}

fn section_header(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("section-header");
    label.set_halign(gtk4::Align::Start);
    label.set_margin_start(8);
    label.set_margin_bottom(4);
    label
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
