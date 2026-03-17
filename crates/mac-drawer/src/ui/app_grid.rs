use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::search::subsequence_match;
use crate::ui::widgets;
use dock_common::desktop::entry::DesktopEntry;
use gtk4::prelude::*;
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
) -> gtk4::FlowBox {
    let flow_box = gtk4::FlowBox::new();
    flow_box.set_min_children_per_line(config.columns);
    flow_box.set_max_children_per_line(config.columns);
    flow_box.set_column_spacing(config.spacing);
    flow_box.set_row_spacing(config.spacing);
    flow_box.set_homogeneous(true);
    flow_box.set_selection_mode(gtk4::SelectionMode::None);

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
            let button = flow_box_button(entry, config, state, pinned_file, Rc::clone(&on_launch));
            flow_box.insert(&button, -1);
        }
    }

    flow_box
}

fn flow_box_button(
    entry: &DesktopEntry,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &std::path::Path,
    on_launch: Rc<dyn Fn()>,
) -> gtk4::Button {
    let app_dirs = state.borrow().app_dirs.clone();
    let name = if !entry.name_loc.is_empty() {
        &entry.name_loc
    } else {
        &entry.name
    };

    let button = widgets::app_icon_button(&entry.icon, name, config.icon_size, &app_dirs);

    // Click → launch
    let exec = entry.exec.clone();
    let terminal = entry.terminal;
    let term_cmd = config.term.clone();
    let on_launch_click = Rc::clone(&on_launch);
    let compositor = Rc::clone(&state.borrow().compositor);
    button.connect_clicked(move |_| {
        let clean = widgets::clean_exec(&exec);
        if !clean.is_empty() {
            if terminal {
                dock_common::launch::launch_terminal_via_compositor(
                    &clean,
                    &term_cmd,
                    &*compositor,
                );
            } else {
                dock_common::launch::launch_via_compositor(&clean, &*compositor);
            }
        }
        on_launch_click();
    });

    // Tooltip
    let desc = widgets::truncate(&entry.comment_loc, 120); // max tooltip length
    if !desc.is_empty() {
        button.set_tooltip_text(Some(&desc));
    }

    // Right-click → pin
    let desktop_id = entry.desktop_id.clone();
    let state_ref = Rc::clone(state);
    let pinned_path = pinned_file.to_path_buf();
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let mut s = state_ref.borrow_mut();
        if dock_common::pinning::pin_item(&mut s.pinned, &desktop_id) {
            let _ = dock_common::pinning::save_pinned(&s.pinned, &pinned_path);
            log::info!("Pinned {}", desktop_id);
        }
    });
    button.add_controller(gesture);

    button
}
