use crate::config::DockConfig;
use crate::state::DockState;
use crate::ui::buttons;
use dock_common::pinning;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Builds the main dock content box with pinned and task buttons.
///
/// This is the core UI builder, called on every refresh.
pub fn build(
    alignment_box: &gtk4::Box,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    data_home: &Path,
    pinned_file: &Path,
    rebuild: &Rc<dyn Fn()>,
    win: &gtk4::ApplicationWindow,
) -> gtk4::Box {
    let inner_orientation = if config.is_vertical() {
        gtk4::Orientation::Vertical
    } else {
        gtk4::Orientation::Horizontal
    };
    let main_box = gtk4::Box::new(inner_orientation, 0);

    // Pack into alignment box — replicate Go's PackStart expand/fill semantics
    // Go: "start" → PackStart(mainBox, false, true, 0)  expand=false, fill=true
    // Go: "end"   → PackEnd(mainBox, false, true, 0)    expand=false, fill=true
    // Go: center  → PackStart(mainBox, true, false, 0)  expand=true,  fill=false
    match config.alignment.as_str() {
        "start" => {
            alignment_box.prepend(&main_box);
            // expand=false: don't take extra space
        }
        "end" => {
            alignment_box.append(&main_box);
            // expand=false: don't take extra space
        }
        _ => {
            // Center: when full-width, expand to fill then center content.
            // When content-width, just append (window shrinks to fit).
            if config.full {
                main_box.set_hexpand(true);
                main_box.set_halign(gtk4::Align::Center);
            }
            alignment_box.append(&main_box);
        }
    }

    let mut s = state.borrow_mut();

    // Reload pinned items
    s.pinned = pinning::load_pinned(pinned_file);

    // Collect all unique items (pinned first, then running tasks)
    let mut all_items: Vec<String> = Vec::new();
    for pin in &s.pinned {
        if !all_items.contains(pin) {
            all_items.push(pin.clone());
        }
    }

    // Sort clients by workspace then class
    s.clients.sort_by(|a, b| {
        a.workspace.id.cmp(&b.workspace.id).then_with(|| a.class.cmp(&b.class))
    });

    // Filter out ignored workspaces
    let ignored_ws = config.ignored_workspaces();
    s.clients.retain(|cl| {
        let ws_base = cl.workspace.name.split(':').next().unwrap_or("");
        !ignored_ws.contains(&cl.workspace.id.to_string())
            && !ignored_ws.iter().any(|iw| iw == ws_base)
    });

    let ignored_classes = config.ignored_classes();

    for task in &s.clients {
        if !all_items.contains(&task.class)
            && !config.launcher_cmd.contains(&task.class)
            && !task.class.is_empty()
        {
            all_items.push(task.class.clone());
        }
    }

    // Scale icons down when too many apps
    let count = all_items.len().max(1);
    if config.icon_size * 6 / (count as i32) < config.icon_size {
        let overflow = (all_items.len() as i32 - 6) / 3;
        s.img_size_scaled = config.icon_size * 6 / (6 + overflow);
    } else {
        s.img_size_scaled = config.icon_size;
    }

    log::debug!(
        "Dock build: {} items, icon_size={}, img_size_scaled={}, pinned={}",
        all_items.len(), config.icon_size, s.img_size_scaled, s.pinned.len()
    );

    drop(s);

    // Launcher at start
    if config.launcher_pos == "start"
        && let Some(btn) = buttons::launcher_button(config, state, data_home, win)
    {
        main_box.append(&btn);
    }

    // Pinned items
    let mut already_added: Vec<String> = Vec::new();
    let pinned_snapshot = state.borrow().pinned.clone();
    let clients_snapshot = state.borrow().clients.clone();
    let active_class = state
        .borrow()
        .active_client
        .as_ref()
        .map(|c| c.class.clone())
        .unwrap_or_default();

    for pin in &pinned_snapshot {
        if ignored_classes.contains(pin) {
            log::debug!("Ignoring pin '{}'", pin);
            continue;
        }

        let instances = state.borrow().task_instances(pin);
        if instances.is_empty() {
            let btn = buttons::pinned_button(pin, config, state, data_home, pinned_file, rebuild);
            main_box.append(&btn);
        } else if instances.len() == 1 || !already_added.contains(pin) {
            let btn = buttons::task_button(
                &instances[0], &instances, config, state, data_home, pinned_file, rebuild,
            );
            if instances[0].class == active_class && !config.autohide {
                btn.set_widget_name("active");
            }
            main_box.append(&btn);
            already_added.push(pin.clone());
        }
    }

    // Running tasks (not pinned)
    already_added.clear();
    for task in &clients_snapshot {
        if task.class.is_empty()
            || pinning::is_pinned(&pinned_snapshot, &task.class)
            || ignored_classes.contains(&task.class)
        {
            continue;
        }

        let instances = state.borrow().task_instances(&task.class);
        if instances.len() == 1 || !already_added.contains(&task.class) {
            let btn = buttons::task_button(
                task, &instances, config, state, data_home, pinned_file, rebuild,
            );
            if task.class == active_class && !config.autohide {
                btn.set_widget_name("active");
            }
            main_box.append(&btn);
            already_added.push(task.class.clone());
        }
    }

    // Launcher at end
    if config.launcher_pos == "end"
        && let Some(btn) = buttons::launcher_button(config, state, data_home, win)
    {
        main_box.append(&btn);
    }

    main_box
}
