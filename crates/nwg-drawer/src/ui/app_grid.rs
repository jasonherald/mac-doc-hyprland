use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::search::subsequence_match;
use crate::ui::widgets;
use gtk4::prelude::*;
use nwg_dock_common::desktop::entry::DesktopEntry;
use nwg_dock_common::pinning;
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
            insert_non_focusable(&flow_box, &button);
        }
    }

    flow_box
}

/// Builds a single unified FlowBox: pinned items first, then all other apps.
///
/// Everything is in one focus group so arrow keys flow naturally through
/// pinned → apps without Tab boundaries. Pinned row is padded with spacers
/// so apps always start on a fresh row. Right-click pin/unpin triggers
/// an immediate rebuild via `on_rebuild`.
#[allow(clippy::too_many_arguments)]
pub fn build_unified_flow_box(
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &std::path::Path,
    on_launch: Rc<dyn Fn()>,
    status_label: &gtk4::Label,
    on_rebuild: Rc<dyn Fn()>,
) -> gtk4::FlowBox {
    let flow_box = create_flow_box(config);
    let pinned = state.borrow().pinned.clone();
    let entries = state.borrow().apps.entries.clone();
    let id2entry = state.borrow().apps.id2entry.clone();

    // 1. Add pinned items
    let mut pinned_count: u32 = 0;
    for desktop_id in &pinned {
        if let Some(entry) = id2entry.get(desktop_id) {
            if entry.desktop_id.is_empty() || entry.no_display {
                continue;
            }
            let button = build_button(
                entry,
                config,
                state,
                pinned_file,
                &on_launch,
                status_label,
                Some(Rc::clone(&on_rebuild)),
            );
            // Right-click on pinned item → unpin
            add_unpin_handler(&button, desktop_id, state, pinned_file, &on_rebuild);
            insert_non_focusable(&flow_box, &button);
            pinned_count += 1;
        }
    }

    // 2. Pad remaining slots in the pinned row with spacers
    if pinned_count > 0 {
        let remainder = pinned_count % config.columns;
        if remainder != 0 {
            let pad = config.columns - remainder;
            for _ in 0..pad {
                let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                spacer.add_css_class("pinned-row-spacer");
                spacer.set_focusable(false);
                spacer.set_can_target(false);
                spacer.set_sensitive(false);
                flow_box.insert(&spacer, -1);
                if let Some(child) = flow_box.last_child() {
                    child.set_focusable(false);
                    child.set_can_target(false);
                    child.set_sensitive(false);
                }
            }
        }
    }

    // 3. Add all non-pinned apps
    for entry in &entries {
        if entry.no_display {
            continue;
        }
        if pinned.iter().any(|p| p == &entry.desktop_id) {
            continue;
        }
        let button = build_button(
            entry,
            config,
            state,
            pinned_file,
            &on_launch,
            status_label,
            Some(Rc::clone(&on_rebuild)),
        );
        // Right-click on app → pin
        add_pin_handler(&button, &entry.desktop_id, state, pinned_file, &on_rebuild);
        insert_non_focusable(&flow_box, &button);
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
fn insert_non_focusable(flow_box: &gtk4::FlowBox, button: &gtk4::Button) {
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

/// Adds a right-click handler that PINS the app and triggers immediate rebuild.
fn add_pin_handler(
    button: &gtk4::Button,
    desktop_id: &str,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &std::path::Path,
    on_rebuild: &Rc<dyn Fn()>,
) {
    let id = desktop_id.to_string();
    let state_ref = Rc::clone(state);
    let path = pinned_file.to_path_buf();
    let rebuild = Rc::clone(on_rebuild);
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let mut s = state_ref.borrow_mut();
        if pinning::pin_item(&mut s.pinned, &id) {
            let _ = pinning::save_pinned(&s.pinned, &path);
            log::info!("Pinned {}", id);
            drop(s);
            rebuild();
        }
    });
    button.add_controller(gesture);
}

/// Adds a right-click handler that UNPINS the app and triggers immediate rebuild.
fn add_unpin_handler(
    button: &gtk4::Button,
    desktop_id: &str,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &std::path::Path,
    on_rebuild: &Rc<dyn Fn()>,
) {
    let id = desktop_id.to_string();
    let state_ref = Rc::clone(state);
    let path = pinned_file.to_path_buf();
    let rebuild = Rc::clone(on_rebuild);
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let mut s = state_ref.borrow_mut();
        if pinning::unpin_item(&mut s.pinned, &id) {
            let _ = pinning::save_pinned(&s.pinned, &path);
            log::info!("Unpinned {}", id);
            drop(s);
            rebuild();
        }
    });
    button.add_controller(gesture);
}
