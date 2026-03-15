use crate::config::DockConfig;
use crate::state::DockState;
use dock_common::hyprland::types::HyprClient;
use dock_common::pinning;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Creates a popover that tracks open/close state to prevent autohide.
fn create_tracked_popover(
    parent: &impl IsA<gtk4::Widget>,
    state: &Rc<RefCell<DockState>>,
) -> gtk4::Popover {
    let popover = gtk4::Popover::new();
    popover.set_parent(parent.upcast_ref());

    let state_open = Rc::clone(state);
    popover.connect_show(move |_| {
        state_open.borrow_mut().popover_open = true;
    });
    let state_close = Rc::clone(state);
    popover.connect_closed(move |_| {
        state_close.borrow_mut().popover_open = false;
    });
    popover
}

/// Creates and shows a popover listing all instances of a class (for multi-instance left-click).
pub fn show_client_menu(
    instances: &[HyprClient],
    state: &Rc<RefCell<DockState>>,
    parent: &impl IsA<gtk4::Widget>,
) {
    let popover = create_tracked_popover(parent, state);

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    for instance in instances {
        let title = truncate_title(&instance.title, 25);
        let label = format!("{} ({})", title, instance.workspace.name);
        let btn = gtk4::Button::with_label(&label);
        btn.add_css_class("flat");

        let addr = instance.address.clone();
        let ws_name = instance.workspace.name.clone();
        let popover_ref = popover.clone();
        btn.connect_clicked(move |_| {
            popover_ref.popdown();
            focus_window(&addr, &ws_name);
        });
        vbox.append(&btn);
    }

    popover.set_child(Some(&vbox));
    popover.popup();
}

/// Creates and shows a context menu for a task (right-click).
pub fn show_context_menu(
    class: &str,
    instances: &[HyprClient],
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    pinned_file: &Path,
    rebuild: &Rc<dyn Fn()>,
    parent: &impl IsA<gtk4::Widget>,
) {
    let popover = create_tracked_popover(parent, state);

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 2);

    // Per-instance actions
    for instance in instances {
        let title = truncate_title(&instance.title, 25);
        let header = gtk4::Label::new(Some(&format!("{} ({})", title, instance.workspace.name)));
        header.add_css_class("heading");
        vbox.append(&header);

        let addr = &instance.address;
        let actions_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        actions_box.append(&action_button("Close", &popover, {
            let a = addr.clone();
            move || {
                let _ = dock_common::hyprland::ipc::hyprctl(&format!(
                    "dispatch closewindow address:{}",
                    a
                ));
            }
        }));
        actions_box.append(&action_button("Toggle Floating", &popover, {
            let a = addr.clone();
            move || {
                let _ = dock_common::hyprland::ipc::hyprctl(&format!(
                    "dispatch togglefloating address:{}",
                    a
                ));
            }
        }));
        actions_box.append(&action_button("Fullscreen", &popover, {
            let a = addr.clone();
            move || {
                let _ = dock_common::hyprland::ipc::hyprctl(&format!(
                    "dispatch fullscreen address:{}",
                    a
                ));
            }
        }));

        for ws in 1..=config.num_ws {
            actions_box.append(&action_button(&format!("-> WS {}", ws), &popover, {
                let a = addr.clone();
                move || {
                    let _ = dock_common::hyprland::ipc::hyprctl(&format!(
                        "dispatch movetoworkspace {},address:{}",
                        ws, a
                    ));
                }
            }));
        }

        vbox.append(&actions_box);
        vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    }

    // New window
    let btn = gtk4::Button::with_label("New window");
    btn.add_css_class("flat");
    let class_str = class.to_string();
    let app_dirs = state.borrow().app_dirs.clone();
    let p = popover.clone();
    btn.connect_clicked(move |_| {
        dock_common::launch::launch(&class_str, &app_dirs);
        p.popdown();
    });
    vbox.append(&btn);

    // Close all
    let btn = gtk4::Button::with_label("Close all windows");
    btn.add_css_class("flat");
    let insts: Vec<String> = instances.iter().map(|i| i.address.clone()).collect();
    let p = popover.clone();
    btn.connect_clicked(move |_| {
        for addr in &insts {
            let _ = dock_common::hyprland::ipc::hyprctl(&format!(
                "dispatch closewindow address:{}",
                addr
            ));
        }
        p.popdown();
    });
    vbox.append(&btn);

    // Pin/Unpin
    let is_pinned = pinning::is_pinned(&state.borrow().pinned, class);
    let btn = if is_pinned {
        gtk4::Button::with_label("Unpin")
    } else {
        gtk4::Button::with_label("Pin")
    };
    btn.add_css_class("flat");
    let class_str = class.to_string();
    let state_ref = Rc::clone(state);
    let pinned_path = pinned_file.to_path_buf();
    let rebuild_ref = Rc::clone(rebuild);
    let p = popover.clone();
    btn.connect_clicked(move |_| {
        let mut s = state_ref.borrow_mut();
        if is_pinned {
            pinning::unpin_item(&mut s.pinned, &class_str);
        } else {
            pinning::pin_item(&mut s.pinned, &class_str);
        }
        let _ = pinning::save_pinned(&s.pinned, &pinned_path);
        drop(s);
        p.popdown();
        rebuild_ref();
    });
    vbox.append(&btn);

    popover.set_child(Some(&vbox));
    popover.popup();
}

/// Creates and shows a simple unpin context menu for pinned-only items.
pub fn show_pinned_context_menu(
    task_id: &str,
    state: &Rc<RefCell<DockState>>,
    pinned_file: &Path,
    rebuild: &Rc<dyn Fn()>,
    parent: &impl IsA<gtk4::Widget>,
) {
    let popover = create_tracked_popover(parent, state);

    let btn = gtk4::Button::with_label("Unpin");
    btn.add_css_class("flat");
    let id = task_id.to_string();
    let state_ref = Rc::clone(state);
    let pinned_path = pinned_file.to_path_buf();
    let rebuild_ref = Rc::clone(rebuild);
    let p = popover.clone();
    btn.connect_clicked(move |_| {
        let mut s = state_ref.borrow_mut();
        pinning::unpin_item(&mut s.pinned, &id);
        let _ = pinning::save_pinned(&s.pinned, &pinned_path);
        drop(s);
        p.popdown();
        rebuild_ref();
    });

    popover.set_child(Some(&btn));
    popover.popup();
}

fn focus_window(address: &str, workspace_name: &str) {
    if workspace_name.starts_with("special") {
        let special_name = workspace_name.strip_prefix("special:").unwrap_or("");
        let _ = dock_common::hyprland::ipc::hyprctl(&format!(
            "dispatch togglespecialworkspace {}",
            special_name
        ));
    } else {
        let _ = dock_common::hyprland::ipc::hyprctl(&format!(
            "dispatch focuswindow address:{}",
            address
        ));
    }
    let _ = dock_common::hyprland::ipc::hyprctl("dispatch bringactivetotop");
}

/// Creates a flat button that runs an action and closes the popover.
fn action_button(
    label: &str,
    popover: &gtk4::Popover,
    action: impl Fn() + 'static,
) -> gtk4::Button {
    let btn = gtk4::Button::with_label(label);
    btn.add_css_class("flat");
    let p = popover.clone();
    btn.connect_clicked(move |_| {
        action();
        p.popdown();
    });
    btn
}

fn truncate_title(title: &str, max: usize) -> String {
    if title.chars().count() > max {
        let truncated: String = title.chars().take(max).collect();
        format!("{}...", truncated)
    } else {
        title.to_string()
    }
}
