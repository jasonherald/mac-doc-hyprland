use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::search::subsequence_match;
use dock_common::desktop::entry::DesktopEntry;
use dock_common::desktop::icons;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Truncates a string to max chars, appending ellipsis if needed.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    } else {
        s.to_string()
    }
}

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

    let entries = state.borrow().desktop_entries.clone();
    let needle = search_phrase.to_lowercase();

    for entry in &entries {
        if entry.no_display {
            continue;
        }

        let show = if search_phrase.is_empty() {
            // No search — show all or filter by category
            match category_filter {
                Some(ids) => ids.iter().any(|id| id == &entry.desktop_id),
                None => true,
            }
        } else {
            // Search mode
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

/// Creates a single app button for the FlowBox.
/// GTK4 buttons need an explicit Box(vertical) to stack icon above label,
/// since GTK3's SetImage/SetImagePosition/SetAlwaysShowImage don't exist.
fn flow_box_button(
    entry: &DesktopEntry,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &std::path::Path,
    on_launch: Rc<dyn Fn()>,
) -> gtk4::Button {
    let button = gtk4::Button::new();
    button.set_has_frame(false);
    button.add_css_class("app-button");

    // Build icon-above-label layout
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    vbox.set_halign(gtk4::Align::Center);

    // Icon
    let app_dirs = state.borrow().app_dirs.clone();
    if !entry.icon.is_empty() {
        if let Some(image) = icons::create_image(&entry.icon, config.icon_size, &app_dirs) {
            image.set_pixel_size(config.icon_size);
            image.set_halign(gtk4::Align::Center);
            vbox.append(&image);
        }
    }

    // Label below icon
    let name = if !entry.name_loc.is_empty() {
        &entry.name_loc
    } else {
        &entry.name
    };
    let label = gtk4::Label::new(Some(&truncate(name, 20)));
    label.set_halign(gtk4::Align::Center);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_max_width_chars(14);
    vbox.append(&label);

    button.set_child(Some(&vbox));

    // Click → launch
    let exec = entry.exec.clone();
    let terminal = entry.terminal;
    let term_cmd = config.term.clone();
    let on_launch_click = Rc::clone(&on_launch);
    button.connect_clicked(move |_| {
        launch_exec(&exec, terminal, &term_cmd);
        on_launch_click();
    });

    // Tooltip from description
    let desc = truncate(&entry.comment_loc, 120);
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

/// Launches a desktop entry's Exec command via hyprctl dispatch exec.
fn launch_exec(exec: &str, terminal: bool, term_cmd: &str) {
    let exec = exec.replace(['"', '\''], "");

    // Strip field codes (%u, %f, %U, %F, etc.)
    let exec = if let Some(pos) = exec.find('%') {
        exec[..pos.saturating_sub(1)].trim().to_string()
    } else {
        exec.trim().to_string()
    };

    if exec.is_empty() {
        return;
    }

    if terminal {
        dock_common::launch::launch_hyprctl_terminal(&exec, term_cmd);
    } else {
        dock_common::launch::launch_hyprctl(&exec);
    }
}
