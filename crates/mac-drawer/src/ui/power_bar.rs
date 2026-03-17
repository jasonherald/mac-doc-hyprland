use crate::config::DrawerConfig;
use dock_common::desktop::icons;
use gtk4::prelude::*;
use std::path::Path;
use std::rc::Rc;

/// SVG filenames and corresponding theme icon names for the power bar.
/// Order: lock, exit, reboot, sleep, poweroff (matches Go original).
const POWER_BUTTONS: &[PowerButtonDef] = &[
    PowerButtonDef {
        svg: "lock.svg",
        theme: "system-lock-screen-symbolic",
    },
    PowerButtonDef {
        svg: "exit.svg",
        theme: "system-log-out-symbolic",
    },
    PowerButtonDef {
        svg: "reboot.svg",
        theme: "system-reboot-symbolic",
    },
    PowerButtonDef {
        svg: "sleep.svg",
        theme: "face-yawn-symbolic",
    },
    PowerButtonDef {
        svg: "poweroff.svg",
        theme: "system-shutdown-symbolic",
    },
];

struct PowerButtonDef {
    svg: &'static str,
    theme: &'static str,
}

/// Builds the power bar with lock/exit/reboot/sleep/poweroff buttons.
///
/// By default, uses built-in SVG icons from the data directory.
/// If `--pb-use-icon-theme` is set, uses the system icon theme instead.
pub fn build_power_bar(
    config: &DrawerConfig,
    on_launch: Rc<dyn Fn()>,
    data_home: Option<&Path>,
) -> gtk4::Box {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    hbox.set_halign(gtk4::Align::Center);

    let commands = [
        &config.pb_lock,
        &config.pb_exit,
        &config.pb_reboot,
        &config.pb_sleep,
        &config.pb_poweroff,
    ];

    for (def, command) in POWER_BUTTONS.iter().zip(commands.iter()) {
        if command.is_empty() {
            continue;
        }

        let button = gtk4::Button::new();
        let image = create_power_icon(def, config, data_home);
        button.set_child(Some(&image));

        let cmd = command.to_string();
        let on_launch = Rc::clone(&on_launch);
        button.connect_clicked(move |_| {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if let Some((&prog, args)) = parts.split_first() {
                let mut command = std::process::Command::new(prog);
                command.args(args);
                if let Err(e) = command.spawn() {
                    log::error!("Failed to run power command '{}': {}", cmd, e);
                }
            }
            on_launch();
        });

        button.set_tooltip_text(Some(command));
        hbox.append(&button);
    }

    hbox
}

/// Creates the icon widget for a power bar button.
/// Tries built-in SVG first (unless --pb-use-icon-theme), falls back to theme icon.
fn create_power_icon(
    def: &PowerButtonDef,
    config: &DrawerConfig,
    data_home: Option<&Path>,
) -> gtk4::Image {
    // If not using icon theme, try built-in SVG from data directory
    if !config.pb_use_icon_theme
        && let Some(home) = data_home
    {
        let svg_path = home.join("nwg-drawer/img").join(def.svg);
        if let Some(pixbuf) = icons::pixbuf_from_file(&svg_path, config.pb_size, config.pb_size) {
            let image = gtk4::Image::from_pixbuf(Some(&pixbuf));
            image.set_pixel_size(config.pb_size);
            return image;
        }
        log::debug!(
            "Built-in power icon '{}' not found, falling back to theme",
            svg_path.display()
        );
    }

    // Fall back to theme icon
    let image = gtk4::Image::from_icon_name(def.theme);
    image.set_pixel_size(config.pb_size);
    image
}
