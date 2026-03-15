use crate::config::DockConfig;
use crate::state::DockState;
use dock_common::desktop::icons;
use dock_common::hyprland::types::HyprClient;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Indicator SVG filenames based on instance count and orientation.
struct IndicatorAsset {
    name: &'static str,
    width_divisor: i32,  // 1 = full size, 8 = 1/8 size
    height_divisor: i32,
}

fn indicator_asset(count: usize, vertical: bool) -> IndicatorAsset {
    match (count, vertical) {
        (0, false) => IndicatorAsset { name: "task-empty.svg", width_divisor: 1, height_divisor: 8 },
        (0, true) => IndicatorAsset { name: "task-empty-vertical.svg", width_divisor: 8, height_divisor: 1 },
        (1, false) => IndicatorAsset { name: "task-single.svg", width_divisor: 1, height_divisor: 8 },
        (1, true) => IndicatorAsset { name: "task-single-vertical.svg", width_divisor: 8, height_divisor: 1 },
        (_, false) => IndicatorAsset { name: "task-multiple.svg", width_divisor: 1, height_divisor: 8 },
        (_, true) => IndicatorAsset { name: "task-multiple-vertical.svg", width_divisor: 8, height_divisor: 1 },
    }
}

/// Creates an indicator image widget (the dot/bar below/beside the icon).
fn indicator_image(
    data_home: &Path,
    count: usize,
    vertical: bool,
    img_size: i32,
) -> Option<gtk4::Image> {
    let asset = indicator_asset(count, vertical);
    let path = data_home.join("nwg-dock-hyprland/images").join(asset.name);
    let w = img_size / asset.width_divisor;
    let h = img_size / asset.height_divisor;
    let pixbuf = icons::pixbuf_from_file(&path, w, h)?;
    Some(gtk4::Image::from_pixbuf(Some(&pixbuf)))
}

/// Packs a button and indicator into a box with correct ordering for position.
fn pack_button_box(
    button: &gtk4::Button,
    indicator: Option<&gtk4::Image>,
    position: &str,
    vertical: bool,
) -> gtk4::Box {
    let orientation = if vertical {
        gtk4::Orientation::Horizontal
    } else {
        gtk4::Orientation::Vertical
    };
    let bx = gtk4::Box::new(orientation, 0);

    let at_start = position == "left" || position == "top";

    if let Some(img) = indicator {
        if at_start {
            bx.append(img);
            bx.append(button);
        } else {
            bx.append(button);
            bx.append(img);
        }
    } else {
        bx.append(button);
    }

    bx
}

/// Creates a pinned app button (not currently running).
pub fn pinned_button(
    app_id: &str,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    data_home: &Path,
) -> gtk4::Box {
    let img_size = state.borrow().img_size_scaled;
    let app_dirs = state.borrow().app_dirs.clone();

    let button = gtk4::Button::new();
    if let Some(image) = icons::create_image(app_id, img_size, &app_dirs) {
        button.set_child(Some(&image));
    } else {
        let path = data_home.join("nwg-dock-hyprland/images/icon-missing.svg");
        if let Some(pb) = icons::pixbuf_from_file(&path, img_size, img_size) {
            button.set_child(Some(&gtk4::Image::from_pixbuf(Some(&pb))));
        }
    }
    button.set_tooltip_text(Some(&icons::get_name(app_id, &app_dirs)));

    // Click → launch
    let id = app_id.to_string();
    let dirs = app_dirs.clone();
    button.connect_clicked(move |_| {
        dock_common::launch::launch(&id, &dirs);
    });

    let indicator = indicator_image(data_home, 0, config.is_vertical(), img_size);
    pack_button_box(&button, indicator.as_ref(), &config.position, config.is_vertical())
}

/// Creates a task button for a running application.
pub fn task_button(
    client: &HyprClient,
    instances: &[HyprClient],
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    data_home: &Path,
) -> gtk4::Box {
    let img_size = state.borrow().img_size_scaled;
    let app_dirs = state.borrow().app_dirs.clone();

    let button = gtk4::Button::new();
    if let Some(image) = icons::create_image(&client.class, img_size, &app_dirs) {
        button.set_child(Some(&image));
    } else {
        let path = data_home.join("nwg-dock-hyprland/images/icon-missing.svg");
        if let Some(pb) = icons::pixbuf_from_file(&path, img_size, img_size) {
            button.set_child(Some(&gtk4::Image::from_pixbuf(Some(&pb))));
        }
    }
    button.set_tooltip_text(Some(&icons::get_name(&client.class, &app_dirs)));

    // Click behavior depends on instance count
    if instances.len() == 1 {
        let addr = client.address.clone();
        let ws_name = client.workspace.name.clone();
        button.connect_clicked(move |_| {
            focus_window(&addr, &ws_name);
        });
    } else {
        // Multiple instances — we'll handle this via menus in the click handler
        // For now, focus the first instance
        let addr = client.address.clone();
        let ws_name = client.workspace.name.clone();
        button.connect_clicked(move |_| {
            focus_window(&addr, &ws_name);
        });
    }

    let indicator = indicator_image(
        data_home,
        instances.len(),
        config.is_vertical(),
        img_size,
    );
    pack_button_box(&button, indicator.as_ref(), &config.position, config.is_vertical())
}

/// Creates the launcher button (opens the drawer).
pub fn launcher_button(
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    data_home: &Path,
) -> Option<gtk4::Box> {
    if config.nolauncher || config.launcher_cmd.is_empty() {
        return None;
    }

    let img_size = state.borrow().img_size_scaled;
    let button = gtk4::Button::new();

    let pixbuf = if config.ico.is_empty() {
        let path = data_home.join("nwg-dock-hyprland/images/grid.svg");
        icons::pixbuf_from_file(&path, img_size, img_size)
    } else {
        icons::create_pixbuf(&config.ico, img_size)
    };

    if let Some(pb) = pixbuf {
        button.set_child(Some(&gtk4::Image::from_pixbuf(Some(&pb))));
    } else {
        return None;
    }

    let cmd = config.launcher_cmd.clone();
    button.connect_clicked(move |_| {
        let elements: Vec<&str> = cmd.split_whitespace().collect();
        if let Some((&prog, args)) = elements.split_first() {
            let mut command = std::process::Command::new(prog);
            command.args(args);
            if let Err(e) = command.spawn() {
                log::warn!("Unable to start launcher: {}", e);
            }
        }
    });

    let indicator = indicator_image(data_home, 0, config.is_vertical(), img_size);
    Some(pack_button_box(
        &button,
        indicator.as_ref(),
        &config.position,
        config.is_vertical(),
    ))
}

/// Focuses a window by address, handling special workspaces.
fn focus_window(address: &str, workspace_name: &str) {
    if workspace_name.starts_with("special") {
        let special_name = workspace_name
            .strip_prefix("special:")
            .unwrap_or("");
        let cmd = format!("dispatch togglespecialworkspace {}", special_name);
        let _ = dock_common::hyprland::ipc::hyprctl(&cmd);
    } else {
        let cmd = format!("dispatch focuswindow address:{}", address);
        let _ = dock_common::hyprland::ipc::hyprctl(&cmd);
    }
    // Bring to top (fix #14 from original)
    let _ = dock_common::hyprland::ipc::hyprctl("dispatch bringactivetotop");
}
