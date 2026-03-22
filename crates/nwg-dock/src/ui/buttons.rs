use crate::context::DockContext;
use crate::ui::menus;
use gtk4::prelude::*;
use nwg_dock_common::compositor::WmClient;
use nwg_dock_common::desktop::icons;
use std::path::Path;
use std::rc::Rc;

/// Indicator SVG filenames based on instance count and orientation.
struct IndicatorAsset {
    name: &'static str,
    width_divisor: i32,
    height_divisor: i32,
}

fn indicator_asset(count: usize, vertical: bool) -> IndicatorAsset {
    match (count, vertical) {
        (0, false) => IndicatorAsset {
            name: "task-empty.svg",
            width_divisor: 1,
            height_divisor: 8,
        },
        (0, true) => IndicatorAsset {
            name: "task-empty-vertical.svg",
            width_divisor: 8,
            height_divisor: 1,
        },
        (1, false) => IndicatorAsset {
            name: "task-single.svg",
            width_divisor: 1,
            height_divisor: 8,
        },
        (1, true) => IndicatorAsset {
            name: "task-single-vertical.svg",
            width_divisor: 8,
            height_divisor: 1,
        },
        (_, false) => IndicatorAsset {
            name: "task-multiple.svg",
            width_divisor: 1,
            height_divisor: 8,
        },
        (_, true) => IndicatorAsset {
            name: "task-multiple-vertical.svg",
            width_divisor: 8,
            height_divisor: 1,
        },
    }
}

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
    let image = gtk4::Image::from_pixbuf(Some(&pixbuf));
    image.add_css_class("dock-indicator");
    Some(image)
}

fn pack_button_box(
    button: &gtk4::Button,
    indicator: Option<&gtk4::Image>,
    position: crate::config::Position,
    vertical: bool,
) -> gtk4::Box {
    let orientation = if vertical {
        gtk4::Orientation::Horizontal
    } else {
        gtk4::Orientation::Vertical
    };
    let bx = gtk4::Box::new(orientation, 0);
    bx.set_margin_start(0);
    bx.set_margin_end(0);
    bx.set_margin_top(0);
    bx.set_margin_bottom(0);

    let at_start = matches!(
        position,
        crate::config::Position::Left | crate::config::Position::Top
    );
    if let Some(img) = indicator {
        img.set_margin_start(0);
        img.set_margin_end(0);
        img.set_margin_top(0);
        img.set_margin_bottom(0);
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
pub fn pinned_button(app_id: &str, index: usize, ctx: &DockContext) -> gtk4::Box {
    let img_size = ctx.state.borrow().img_size_scaled;
    let app_dirs = ctx.state.borrow().app_dirs.clone();

    let button = gtk4::Button::new();
    let image = icons::create_image(app_id, img_size, &app_dirs).unwrap_or_else(|| {
        let path = ctx
            .data_home
            .join("nwg-dock-hyprland/images/icon-missing.svg");
        icons::pixbuf_from_file(&path, img_size, img_size)
            .map(|pb| gtk4::Image::from_pixbuf(Some(&pb)))
            .unwrap_or_default()
    });
    image.set_pixel_size(img_size);
    button.set_child(Some(&image));
    button.add_css_class("dock-button");
    button.set_has_frame(false);
    button.set_tooltip_text(Some(&icons::get_name(app_id, &app_dirs)));

    // Left-click → launch
    let id = app_id.to_string();
    let dirs = app_dirs.clone();
    button.connect_clicked(move |_| {
        nwg_dock_common::launch::launch(&id, &dirs);
    });

    // Right-click → unpin context menu
    let id = app_id.to_string();
    let state_ref = Rc::clone(&ctx.state);
    let pinned_path = ctx.pinned_file.as_ref().clone();
    let rebuild_ref = Rc::clone(&ctx.rebuild);
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        if let Some(widget) = gesture.widget() {
            menus::show_pinned_context_menu(&id, &state_ref, &pinned_path, &rebuild_ref, &widget);
        }
    });
    button.add_controller(gesture);

    let indicator = indicator_image(&ctx.data_home, 0, ctx.config.is_vertical(), img_size);
    let item_box = pack_button_box(
        &button,
        indicator.as_ref(),
        ctx.config.position,
        ctx.config.is_vertical(),
    );
    item_box.add_css_class("dock-item");
    item_box.add_css_class("pinned-item");

    // Manual drag-to-reorder (when dock is unlocked)
    if !ctx.state.borrow().locked {
        crate::ui::drag::setup_drag_gesture(
            &button,
            index,
            ctx.config.is_vertical(),
            &ctx.state,
            &ctx.pinned_file,
            &ctx.rebuild,
        );
    }

    item_box
}

/// Creates a task button for a running application.
pub fn task_button(client: &WmClient, instances: &[WmClient], ctx: &DockContext) -> gtk4::Box {
    let img_size = ctx.state.borrow().img_size_scaled;
    let app_dirs = ctx.state.borrow().app_dirs.clone();

    let button = gtk4::Button::new();
    let image = icons::create_image(&client.class, img_size, &app_dirs).unwrap_or_else(|| {
        let path = ctx
            .data_home
            .join("nwg-dock-hyprland/images/icon-missing.svg");
        icons::pixbuf_from_file(&path, img_size, img_size)
            .map(|pb| gtk4::Image::from_pixbuf(Some(&pb)))
            .unwrap_or_default()
    });
    image.set_pixel_size(img_size);
    button.set_child(Some(&image));
    button.add_css_class("dock-button");
    button.set_has_frame(false);
    button.set_tooltip_text(Some(&icons::get_name(&client.class, &app_dirs)));

    // Left-click
    if instances.len() == 1 {
        let id = client.id.clone();
        let ws_name = client.workspace.name.clone();
        let comp = Rc::clone(&ctx.compositor);
        button.connect_clicked(move |_| {
            menus::focus_window(&id, &ws_name, &*comp);
        });
    } else {
        let insts = instances.to_vec();
        let state_menu = Rc::clone(&ctx.state);
        let comp = Rc::clone(&ctx.compositor);
        button.connect_clicked(move |btn| {
            menus::show_client_menu(&insts, &state_menu, &comp, btn);
        });
    }

    // Middle-click → launch new instance
    let class = client.class.clone();
    let dirs = app_dirs.clone();
    let middle = gtk4::GestureClick::new();
    middle.set_button(2);
    middle.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        nwg_dock_common::launch::launch(&class, &dirs);
    });
    button.add_controller(middle);

    // Right-click → context menu
    let class = client.class.clone();
    let insts = instances.to_vec();
    let config_ref = ctx.config.as_ref().clone();
    let state_ref = Rc::clone(&ctx.state);
    let comp = Rc::clone(&ctx.compositor);
    let pinned_path = ctx.pinned_file.as_ref().clone();
    let rebuild_ref = Rc::clone(&ctx.rebuild);
    let right = gtk4::GestureClick::new();
    right.set_button(3);
    right.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        if let Some(widget) = gesture.widget() {
            menus::show_context_menu(
                &class,
                &insts,
                &config_ref,
                &state_ref,
                &comp,
                &pinned_path,
                &rebuild_ref,
                &widget,
            );
        }
    });
    button.add_controller(right);

    let indicator = indicator_image(
        &ctx.data_home,
        instances.len(),
        ctx.config.is_vertical(),
        img_size,
    );
    pack_button_box(
        &button,
        indicator.as_ref(),
        ctx.config.position,
        ctx.config.is_vertical(),
    )
}

/// Creates the launcher button (opens the drawer).
pub fn launcher_button(ctx: &DockContext, win: &gtk4::ApplicationWindow) -> Option<gtk4::Box> {
    if ctx.config.nolauncher || ctx.config.launcher_cmd.is_empty() {
        return None;
    }

    let img_size = ctx.state.borrow().img_size_scaled;
    let button = gtk4::Button::new();

    let pixbuf = if ctx.config.ico.is_empty() {
        let path = ctx.data_home.join("nwg-dock-hyprland/images/grid.svg");
        icons::pixbuf_from_file(&path, img_size, img_size)
    } else {
        icons::create_pixbuf(&ctx.config.ico, img_size)
    };

    let pb = pixbuf?;
    let image = gtk4::Image::from_pixbuf(Some(&pb));
    image.set_pixel_size(img_size);
    button.set_child(Some(&image));
    button.add_css_class("dock-button");
    button.set_has_frame(false);

    let cmd = ctx.config.launcher_cmd.clone();
    let autohide = ctx.config.autohide;
    let win_ref = win.clone();
    button.connect_clicked(move |_| {
        // Use sh -c to handle quoted arguments in the launcher command
        // (e.g. nwg-drawer --pblock "swaylock" --pbexit "loginctl terminate-session")
        if let Err(e) = std::process::Command::new("sh").args(["-c", &cmd]).spawn() {
            log::warn!("Unable to start launcher: {}", e);
        }
        if autohide {
            win_ref.set_visible(false);
        }
    });

    let indicator = indicator_image(&ctx.data_home, 0, ctx.config.is_vertical(), img_size);
    Some(pack_button_box(
        &button,
        indicator.as_ref(),
        ctx.config.position,
        ctx.config.is_vertical(),
    ))
}
