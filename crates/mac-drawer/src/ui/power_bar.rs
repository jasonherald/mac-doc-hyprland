use crate::config::DrawerConfig;
use gtk4::prelude::*;
use std::rc::Rc;

/// Builds the power bar with lock/exit/reboot/sleep/poweroff buttons.
pub fn build_power_bar(config: &DrawerConfig, on_launch: Rc<dyn Fn()>) -> gtk4::Box {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    hbox.set_halign(gtk4::Align::Center);

    let buttons = [
        ("system-lock-screen", &config.pb_lock),
        ("system-log-out", &config.pb_exit),
        ("system-reboot", &config.pb_reboot),
        ("system-suspend", &config.pb_sleep),
        ("system-shutdown", &config.pb_poweroff),
    ];

    for (icon_name, command) in &buttons {
        if command.is_empty() {
            continue;
        }

        let button = gtk4::Button::new();
        let image = gtk4::Image::from_icon_name(icon_name);
        image.set_pixel_size(config.pb_size);
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
