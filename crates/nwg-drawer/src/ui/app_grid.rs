use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::search::subsequence_match;
use crate::ui::widgets;
use gtk4::prelude::*;
use nwg_dock_common::desktop::entry::DesktopEntry;
use std::cell::RefCell;
use std::rc::Rc;

/// Creates the app FlowBox with optional category filter and search.
pub fn build_app_flow_box(
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    category_filter: Option<&[String]>,
    search_phrase: &str,
    pinned_file: &std::path::Path,
    on_launch: Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) -> gtk4::FlowBox {
    let flow_box = create_flow_box(config);
    let entries = state.borrow().apps.entries.clone();
    let needle = search_phrase.to_lowercase();

    for entry in &entries {
        if entry.no_display {
            continue;
        }

        let show = if search_phrase.is_empty() {
            match category_filter {
                Some(ids) => ids.iter().any(|id| id == &entry.desktop_id),
                None => true,
            }
        } else {
            subsequence_match(&needle, &entry.name_loc)
                || entry.comment_loc.to_lowercase().contains(&needle)
                || entry.comment.to_lowercase().contains(&needle)
                || entry.exec.to_lowercase().contains(&needle)
        };

        if show {
            let button = build_button(
                entry,
                config,
                state,
                pinned_file,
                &on_launch,
                status_label,
                None,
            );
            insert_into_flow(&flow_box, &button);
        }
    }

    flow_box
}

/// Creates a standard FlowBox with the shared configuration.
fn create_flow_box(config: &DrawerConfig) -> gtk4::FlowBox {
    let flow_box = gtk4::FlowBox::new();
    flow_box.set_min_children_per_line(config.columns);
    flow_box.set_max_children_per_line(config.columns);
    flow_box.set_column_spacing(config.spacing);
    flow_box.set_row_spacing(config.spacing);
    flow_box.set_homogeneous(true);
    flow_box.set_selection_mode(gtk4::SelectionMode::None);
    flow_box
}

/// Inserts a button into a FlowBox with a non-focusable FlowBoxChild wrapper.
/// Navigation is handled by our capture-phase controller, not FlowBox internals.
fn insert_into_flow(flow_box: &gtk4::FlowBox, button: &gtk4::Button) {
    flow_box.insert(button, -1);
    if let Some(child) = flow_box.last_child() {
        child.set_focusable(false);
    }
}

/// Builds an app button with click-to-launch and optional right-click rebuild.
fn build_button(
    entry: &DesktopEntry,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    _pinned_file: &std::path::Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
    _on_rebuild: Option<Rc<dyn Fn()>>,
) -> gtk4::Button {
    let app_dirs = state.borrow().app_dirs.clone();
    let name = if !entry.name_loc.is_empty() {
        &entry.name_loc
    } else {
        &entry.name
    };
    let desc = if !entry.comment_loc.is_empty() {
        &entry.comment_loc
    } else {
        &entry.comment
    };

    let button = widgets::app_icon_button(
        &entry.icon,
        name,
        config.icon_size,
        &app_dirs,
        status_label,
        desc,
    );

    // Click → launch
    let exec = entry.exec.clone();
    let terminal = entry.terminal;
    let term_cmd = config.term.clone();
    let on_launch_click = Rc::clone(on_launch);
    let compositor = Rc::clone(&state.borrow().compositor);
    let theme_prefix = state.borrow().gtk_theme_prefix.clone();
    button.connect_clicked(move |_| {
        let clean = widgets::clean_exec(&exec);
        if !clean.is_empty() {
            let cmd = widgets::prepend_theme(&clean, &theme_prefix);
            if terminal {
                nwg_dock_common::launch::launch_terminal_via_compositor(
                    &cmd,
                    &term_cmd,
                    &*compositor,
                );
            } else {
                nwg_dock_common::launch::launch_via_compositor(&cmd, &*compositor);
            }
            on_launch_click();
        }
    });

    // Tooltip
    let tooltip = widgets::truncate(desc, 120);
    if !tooltip.is_empty() {
        button.set_tooltip_text(Some(&tooltip));
    }

    button
}
