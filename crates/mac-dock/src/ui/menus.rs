// Context menu functions — will be wired to button right-click handlers
// when GTK4 gesture controllers are added.
#![allow(dead_code)]

use dock_common::hyprland::types::HyprClient;
use dock_common::pinning;
use gtk4::gio;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::state::DockState;

/// Creates a popover menu listing all instances of a class (for multi-instance tasks).
pub fn client_menu(
    class: &str,
    instances: &[HyprClient],
    parent: &gtk4::Widget,
) -> gtk4::PopoverMenu {
    let menu = gio::Menu::new();

    for (i, instance) in instances.iter().enumerate() {
        let title = truncate_title(&instance.title, 25);
        let label = format!("{} ({})", title, instance.workspace.name);
        let action_name = format!("dock.focus-{}-{}", class, i);
        menu.append(Some(&label), Some(&format!("win.{}", action_name)));
    }

    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(parent);
    popover
}

/// Creates a context menu for a task with window management actions.
pub fn client_context_menu(
    class: &str,
    instances: &[HyprClient],
    num_ws: i32,
    state: &Rc<RefCell<DockState>>,
    parent: &impl IsA<gtk4::Widget>,
) -> gtk4::PopoverMenu {
    let menu = gio::Menu::new();

    // Per-instance sub-sections
    for instance in instances {
        let title = truncate_title(&instance.title, 25);
        let label = format!("{} ({})", title, instance.workspace.name);
        let section = gio::Menu::new();
        section.append(Some(&label), None);
        section.append(Some("Close"), Some(&format!("dock.close.{}", instance.address)));
        section.append(
            Some("Toggle Floating"),
            Some(&format!("dock.float.{}", instance.address)),
        );
        section.append(
            Some("Fullscreen"),
            Some(&format!("dock.fullscreen.{}", instance.address)),
        );

        // Workspace move submenu
        let ws_menu = gio::Menu::new();
        for ws in 1..=num_ws {
            ws_menu.append(
                Some(&format!("→ WS {}", ws)),
                Some(&format!("dock.movews.{}.{}", instance.address, ws)),
            );
        }
        section.append_submenu(Some("Move to..."), &ws_menu);
        menu.append_section(None, &section);
    }

    // Common actions
    let common = gio::Menu::new();
    common.append(Some("New window"), Some(&format!("dock.launch.{}", class)));
    common.append(Some("Close all windows"), Some(&format!("dock.closeall.{}", class)));

    let is_pinned = pinning::is_pinned(&state.borrow().pinned, class);
    if is_pinned {
        common.append(Some("Unpin"), Some(&format!("dock.unpin.{}", class)));
    } else {
        common.append(Some("Pin"), Some(&format!("dock.pin.{}", class)));
    }
    menu.append_section(None, &common);

    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(parent.upcast_ref());
    popover
}

/// Creates a simple context menu for pinned-only items (not running).
pub fn pinned_context_menu(
    task_id: &str,
    parent: &impl IsA<gtk4::Widget>,
) -> gtk4::PopoverMenu {
    let menu = gio::Menu::new();
    menu.append(Some("Unpin"), Some(&format!("dock.unpin.{}", task_id)));

    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(parent.upcast_ref());
    popover
}

/// Installs GAction handlers for dock menu actions on the window.
pub fn install_actions(
    window: &gtk4::ApplicationWindow,
    state: &Rc<RefCell<DockState>>,
    pinned_file: &Path,
    rebuild_fn: Rc<dyn Fn()>,
) {
    let action_group = gio::SimpleActionGroup::new();

    // Generic dispatch action — handles close, float, fullscreen, movews, launch, pin, unpin
    let state_ref = Rc::clone(state);
    let pinned_path = pinned_file.to_path_buf();
    let rebuild = Rc::clone(&rebuild_fn);

    // We use a single parameterized action that parses the action name
    let dispatch = gio::SimpleAction::new("dispatch", Some(&String::static_variant_type()));
    dispatch.connect_activate(move |_, param| {
        if let Some(cmd) = param.and_then(|p| p.get::<String>()) {
            handle_dock_action(&cmd, &state_ref, &pinned_path, &rebuild);
        }
    });
    action_group.add_action(&dispatch);

    window.insert_action_group("dock", Some(&action_group));
}

fn handle_dock_action(
    cmd: &str,
    state: &Rc<RefCell<DockState>>,
    pinned_path: &Path,
    rebuild: &Rc<dyn Fn()>,
) {
    let parts: Vec<&str> = cmd.splitn(3, '.').collect();
    if parts.is_empty() {
        return;
    }

    match parts[0] {
        "close" if parts.len() >= 2 => {
            let addr = parts[1];
            let cmd = format!("dispatch closewindow address:{}", addr);
            let _ = dock_common::hyprland::ipc::hyprctl(&cmd);
        }
        "float" if parts.len() >= 2 => {
            let addr = parts[1];
            let cmd = format!("dispatch togglefloating address:{}", addr);
            let _ = dock_common::hyprland::ipc::hyprctl(&cmd);
        }
        "fullscreen" if parts.len() >= 2 => {
            let addr = parts[1];
            let cmd = format!("dispatch fullscreen address:{}", addr);
            let _ = dock_common::hyprland::ipc::hyprctl(&cmd);
        }
        "movews" if parts.len() >= 2 => {
            // format: movews.ADDRESS.WS_NUM
            let rest: Vec<&str> = parts[1].splitn(2, '.').collect();
            if rest.len() == 2 {
                let cmd = format!("dispatch movetoworkspace {},address:{}", rest[1], rest[0]);
                let _ = dock_common::hyprland::ipc::hyprctl(&cmd);
            }
        }
        "launch" if parts.len() >= 2 => {
            let class = parts[1];
            let app_dirs = state.borrow().app_dirs.clone();
            dock_common::launch::launch(class, &app_dirs);
        }
        "closeall" if parts.len() >= 2 => {
            let class = parts[1];
            let instances = state.borrow().task_instances(class);
            for inst in &instances {
                let cmd = format!("dispatch closewindow address:{}", inst.address);
                let _ = dock_common::hyprland::ipc::hyprctl(&cmd);
            }
        }
        "pin" if parts.len() >= 2 => {
            let class = parts[1];
            let mut s = state.borrow_mut();
            if pinning::pin_item(&mut s.pinned, class) {
                let _ = pinning::save_pinned(&s.pinned, pinned_path);
            }
            drop(s);
            rebuild();
        }
        "unpin" if parts.len() >= 2 => {
            let class = parts[1];
            let mut s = state.borrow_mut();
            if pinning::unpin_item(&mut s.pinned, class) {
                let _ = pinning::save_pinned(&s.pinned, pinned_path);
            }
            drop(s);
            rebuild();
        }
        _ => {
            log::warn!("Unknown dock action: {}", cmd);
        }
    }
}

fn truncate_title(title: &str, max: usize) -> String {
    if title.len() > max {
        format!("{}…", &title[..max])
    } else {
        title.to_string()
    }
}
