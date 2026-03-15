use crate::context::DockContext;
use crate::ui::buttons;
use dock_common::pinning;
use gtk4::prelude::*;
use std::rc::Rc;

/// Builds the main dock content box with pinned and task buttons.
///
/// This is the core UI builder, called on every refresh.
pub fn build(
    alignment_box: &gtk4::Box,
    ctx: &DockContext,
    win: &gtk4::ApplicationWindow,
) -> gtk4::Box {
    let config = &ctx.config;
    let inner_orientation = if config.is_vertical() {
        gtk4::Orientation::Vertical
    } else {
        gtk4::Orientation::Horizontal
    };
    let main_box = gtk4::Box::new(inner_orientation, 0);

    match config.alignment {
        crate::config::Alignment::Start => alignment_box.prepend(&main_box),
        crate::config::Alignment::End => alignment_box.append(&main_box),
        _ => {
            if config.full {
                main_box.set_hexpand(true);
                main_box.set_halign(gtk4::Align::Center);
            }
            alignment_box.append(&main_box);
        }
    }

    let mut s = ctx.state.borrow_mut();
    s.pinned = pinning::load_pinned(&ctx.pinned_file);

    let mut all_items: Vec<String> = Vec::new();
    for pin in &s.pinned {
        if !all_items.contains(pin) {
            all_items.push(pin.clone());
        }
    }

    s.clients.sort_by(|a, b| {
        a.workspace
            .id
            .cmp(&b.workspace.id)
            .then_with(|| a.class.cmp(&b.class))
    });

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
        all_items.len(),
        config.icon_size,
        s.img_size_scaled,
        s.pinned.len()
    );

    drop(s);

    // Launcher at start
    if config.launcher_pos == crate::config::Alignment::Start
        && let Some(btn) = buttons::launcher_button(ctx, win)
    {
        main_box.append(&btn);
    }

    // Pinned items
    let mut already_added: Vec<String> = Vec::new();
    let pinned_snapshot = ctx.state.borrow().pinned.clone();
    let clients_snapshot = ctx.state.borrow().clients.clone();
    let active_class = ctx
        .state
        .borrow()
        .active_client
        .as_ref()
        .map(|c| c.class.clone())
        .unwrap_or_default();

    for (pin_idx, pin) in pinned_snapshot.iter().enumerate() {
        if ignored_classes.contains(pin) {
            continue;
        }
        let instances = ctx.state.borrow().task_instances(pin);
        if instances.is_empty() {
            main_box.append(&buttons::pinned_button(pin, pin_idx, ctx));
        } else if instances.len() == 1 || !already_added.contains(pin) {
            let btn = buttons::task_button(&instances[0], &instances, ctx);
            if instances[0].class == active_class && !config.autohide {
                btn.set_widget_name("active");
            }
            if !ctx.state.borrow().locked
                && let Some(inner_btn) = find_child_button(&btn)
            {
                crate::ui::drag::setup_drag_source(
                    &inner_btn,
                    pin_idx,
                    &ctx.state,
                    &ctx.pinned_file,
                    &ctx.rebuild,
                );
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
        let instances = ctx.state.borrow().task_instances(&task.class);
        if instances.len() == 1 || !already_added.contains(&task.class) {
            let btn = buttons::task_button(task, &instances, ctx);
            if task.class == active_class && !config.autohide {
                btn.set_widget_name("active");
            }
            main_box.append(&btn);
            already_added.push(task.class.clone());
        }
    }

    // Launcher at end
    if config.launcher_pos == crate::config::Alignment::End
        && let Some(btn) = buttons::launcher_button(ctx, win)
    {
        main_box.append(&btn);
    }

    // Right-click dock background → dock settings menu
    let state_bg = Rc::clone(&ctx.state);
    let rebuild_bg = Rc::clone(&ctx.rebuild);
    let bg_gesture = gtk4::GestureClick::new();
    bg_gesture.set_button(3);
    bg_gesture.connect_released(move |gesture, _, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        if let Some(widget) = gesture.widget() {
            crate::ui::dock_menu::show_dock_background_menu(
                &state_bg,
                &rebuild_bg,
                &widget,
                x as i32,
                y as i32,
            );
        }
    });
    main_box.add_controller(bg_gesture);

    // Dock-level drop target for drag-to-reorder (when unlocked)
    if !config.autohide || !ctx.state.borrow().locked {
        crate::ui::drag::setup_dock_drop_target(
            &main_box,
            ctx.state.borrow().img_size_scaled,
            &ctx.state,
            &ctx.pinned_file,
            &ctx.rebuild,
        );
    }

    main_box
}

/// Finds the Button widget inside a dock item box (which may also contain an indicator).
fn find_child_button(item_box: &gtk4::Box) -> Option<gtk4::Button> {
    let mut child = item_box.first_child();
    while let Some(widget) = child {
        if let Ok(btn) = widget.clone().downcast::<gtk4::Button>() {
            return Some(btn);
        }
        child = widget.next_sibling();
    }
    None
}
