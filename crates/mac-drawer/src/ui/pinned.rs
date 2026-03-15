use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::app_grid::truncate;
use dock_common::desktop::icons;
use dock_common::pinning;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Builds the pinned items FlowBox.
pub fn build_pinned_flow_box(
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: Rc<dyn Fn()>,
) -> gtk4::FlowBox {
    let flow_box = gtk4::FlowBox::new();

    let pinned = state.borrow().pinned.clone();
    let count = pinned.len() as u32;
    if count >= config.columns {
        flow_box.set_max_children_per_line(config.columns);
    } else if count > 0 {
        flow_box.set_max_children_per_line(count);
    }
    flow_box.set_column_spacing(config.spacing);
    flow_box.set_row_spacing(config.spacing);
    flow_box.set_homogeneous(true);
    flow_box.set_widget_name("pinned-box");
    flow_box.set_selection_mode(gtk4::SelectionMode::None);

    let id2entry = state.borrow().id2entry.clone();
    let app_dirs = state.borrow().app_dirs.clone();

    for desktop_id in &pinned {
        let entry = match id2entry.get(desktop_id) {
            Some(e) if !e.desktop_id.is_empty() => e,
            _ => {
                log::debug!("Pinned item doesn't seem to exist: {}", desktop_id);
                continue;
            }
        };

        let button = gtk4::Button::new();
        button.set_has_frame(false);
        button.add_css_class("app-button");

        // Icon above label layout
        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        vbox.set_halign(gtk4::Align::Center);

        if !entry.icon.is_empty() {
            if let Some(image) = icons::create_image(&entry.icon, config.icon_size, &app_dirs) {
                image.set_pixel_size(config.icon_size);
                image.set_halign(gtk4::Align::Center);
                vbox.append(&image);
            }
        }

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

        // Left click → launch via hyprctl dispatch
        let exec = entry.exec.clone();
        let terminal = entry.terminal;
        let term = config.term.clone();
        let on_launch_ref = Rc::clone(&on_launch);
        button.connect_clicked(move |_| {
            let exec = exec.replace(['"', '\''], "");
            // Strip field codes (%u, %f, etc.)
            let exec = if let Some(pos) = exec.find('%') {
                exec[..pos.saturating_sub(1)].trim().to_string()
            } else {
                exec.trim().to_string()
            };
            if !exec.is_empty() {
                if terminal {
                    dock_common::launch::launch_hyprctl_terminal(&exec, &term);
                } else {
                    dock_common::launch::launch_hyprctl(&exec);
                }
            }
            on_launch_ref();
        });

        // Right-click gesture → unpin
        let id = desktop_id.clone();
        let state_ref = Rc::clone(state);
        let pinned_path = pinned_file.to_path_buf();
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3); // right click
        gesture.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            let mut s = state_ref.borrow_mut();
            if pinning::unpin_item(&mut s.pinned, &id) {
                let _ = pinning::save_pinned(&s.pinned, &pinned_path);
                log::info!("Unpinned {}", id);
            }
        });
        button.add_controller(gesture);

        flow_box.insert(&button, -1);
    }

    flow_box
}
