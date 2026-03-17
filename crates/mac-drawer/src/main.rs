mod config;
mod desktop_loader;
mod listeners;
mod state;
mod ui;
mod watcher;

use crate::config::DrawerConfig;
use crate::state::DrawerState;
use clap::Parser;
use dock_common::config::paths;
use dock_common::desktop::dirs::get_app_dirs;
use dock_common::pinning;
use dock_common::signals;
use dock_common::singleton;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Mac-style drawer CSS, embedded at compile time.
const DRAWER_CSS: &str = include_str!("assets/drawer.css");

fn main() {
    let config = DrawerConfig::parse();

    if config.debug {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::init();
    }

    // Handle --open/--close: send signal to running instance and exit
    if config.open || config.close {
        if let Some(pid) = singleton::find_running_pid("mac-drawer") {
            let sig = if config.open {
                signals::sig_show()
            } else {
                signals::sig_hide()
            };
            signals::send_signal_to_pid(pid, sig);
            log::info!(
                "Sent {} signal to running instance (pid {})",
                if config.open { "show" } else { "hide" },
                pid
            );
        } else {
            log::warn!("No running drawer instance found");
        }
        std::process::exit(0);
    }

    let _lock = match singleton::acquire_lock("mac-drawer") {
        Ok(lock) => lock,
        Err(existing_pid) => {
            if let Some(pid) = existing_pid {
                if config.resident {
                    log::warn!("Resident instance already running (pid {})", pid);
                } else {
                    signals::send_signal_to_pid(pid, signals::sig_toggle());
                    log::info!("Sent toggle signal to running instance (pid {}), bye!", pid);
                }
            }
            std::process::exit(0);
        }
    };

    let wm_override = if config.wm.is_empty() {
        None
    } else {
        Some(config.wm.as_str())
    };
    let compositor_kind = match dock_common::compositor::detect(wm_override) {
        Ok(k) => k,
        Err(e) => {
            log::error!("{}", e);
            std::process::exit(1);
        }
    };
    let compositor: Rc<dyn dock_common::compositor::Compositor> =
        match dock_common::compositor::create(compositor_kind) {
            Ok(c) => Rc::from(c),
            Err(e) => {
                log::error!("{}", e);
                std::process::exit(1);
            }
        };

    let sig_rx = Rc::new(signals::setup_signal_handlers(config.resident));
    let config_dir = paths::config_dir("nwg-drawer");
    if let Err(e) = paths::ensure_dir(&config_dir) {
        log::warn!("Failed to create config dir: {}", e);
    }

    let cache_dir = paths::cache_dir().expect("Couldn't determine cache directory");
    let pinned_file = cache_dir.join("mac-dock-pinned");
    let css_path = if config.css_file.starts_with('/') {
        std::path::PathBuf::from(&config.css_file)
    } else {
        config_dir.join(&config.css_file)
    };

    if !css_path.exists()
        && let Some(data_dir) = paths::find_data_home("nwg-drawer")
    {
        let src = data_dir.join("nwg-drawer/drawer.css");
        if let Err(e) = paths::copy_file(&src, &css_path) {
            log::warn!("Failed copying default CSS: {}", e);
        }
    }

    let app_dirs = get_app_dirs();
    let exclusions = paths::load_text_lines(&config_dir.join("excluded-dirs")).unwrap_or_default();
    let data_home = paths::find_data_home("nwg-drawer");

    let app = gtk4::Application::builder()
        .application_id("com.mac-drawer.hyprland")
        .build();

    let config = Rc::new(config);
    let pinned_file = Rc::new(pinned_file);
    let css_path = Rc::new(css_path);
    let data_home = Rc::new(data_home);

    app.connect_activate(move |app| {
        let config = Rc::clone(&config);
        let pinned_file = Rc::clone(&pinned_file);

        // CSS
        dock_common::config::css::load_css(&css_path);
        dock_common::config::css::load_css_from_data(DRAWER_CSS);
        apply_theme_settings(&config);

        // State
        let state = Rc::new(RefCell::new(DrawerState::new(
            app_dirs.clone(),
            Rc::clone(&compositor),
        )));
        state.borrow_mut().exclusions = exclusions.clone();
        desktop_loader::load_desktop_entries(&mut state.borrow_mut());
        load_preferred_apps(&mut state.borrow_mut());
        state.borrow_mut().pinned = pinning::load_pinned(&pinned_file);

        // Window
        let win = gtk4::ApplicationWindow::new(app);
        let target_monitor = resolve_target_monitor(&config, &compositor);
        ui::window::setup_drawer_window(&win, &config, target_monitor.as_ref());

        // Layout
        let main_vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        win.set_child(Some(&main_vbox));

        setup_close_button(&main_vbox, &win, &config);

        let search_entry = ui::search::setup_search_entry();
        search_entry.add_css_class("drawer-search");
        search_entry.set_hexpand(false);
        search_entry.set_halign(gtk4::Align::Center);
        search_entry.set_width_request(ui::constants::SEARCH_ENTRY_WIDTH);
        search_entry.set_margin_top(ui::constants::SEARCH_TOP_MARGIN);
        main_vbox.append(&search_entry);

        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);
        main_vbox.append(&scrolled);

        let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content_box.set_margin_top(ui::constants::CONTENT_TOP_MARGIN);
        scrolled.set_child(Some(&content_box));

        let status_label = gtk4::Label::new(None);
        status_label.add_css_class("status-label");

        // On-launch callback
        let on_launch: Rc<dyn Fn()> = {
            let win = win.clone();
            let config = Rc::clone(&config);
            let search_entry = search_entry.clone();
            Rc::new(move || {
                if !config.resident {
                    win.close();
                } else {
                    search_entry.set_text("");
                    win.set_visible(false);
                }
            })
        };

        // Well
        let well = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        well.add_css_class("section-well");
        well.set_halign(gtk4::Align::Center);
        well.set_width_request(ui::constants::WELL_WIDTH);
        content_box.append(&well);

        ui::well_builder::build_normal_well(&well, &config, &state, &pinned_file, &on_launch);

        // Search
        ui::search_handler::connect_search(
            &search_entry,
            &well,
            &status_label,
            &config,
            &state,
            &pinned_file,
            &on_launch,
        );

        // Power bar + status
        if config.has_power_bar() {
            main_vbox.append(&ui::power_bar::build_power_bar(
                &config,
                Rc::clone(&on_launch),
                data_home.as_deref(),
            ));
        }
        main_vbox.append(&status_label);

        // Listeners
        listeners::setup_keyboard(&win, &search_entry, &config, &on_launch, app, &compositor);
        listeners::setup_focus_detector(&win, &on_launch, &compositor);
        listeners::setup_file_watcher(&app_dirs, &pinned_file, &well, &config, &state, &on_launch);
        listeners::setup_signal_poller(&win, &sig_rx);

        win.present();
    });

    app.run_with_args::<String>(&[]);
}

fn apply_theme_settings(config: &DrawerConfig) {
    if let Some(settings) = gtk4::Settings::default() {
        if !config.gtk_theme.is_empty() {
            settings.set_gtk_theme_name(Some(&config.gtk_theme));
            log::info!("Using theme: {}", config.gtk_theme);
        } else {
            settings.set_property("gtk-application-prefer-dark-theme", true);
        }
        if !config.icon_theme.is_empty() {
            settings.set_gtk_icon_theme_name(Some(&config.icon_theme));
            log::info!("Using icon theme: {}", config.icon_theme);
        }
    }
}

fn load_preferred_apps(state: &mut DrawerState) {
    let pa_file = paths::config_dir("nwg-drawer").join("preferred-apps.json");
    if pa_file.exists()
        && let Some(pa) = dock_common::desktop::preferred_apps::load_preferred_apps(&pa_file)
    {
        log::info!("Found {} custom file associations", pa.len());
        state.preferred_apps = pa;
    }
}

fn resolve_target_monitor(
    config: &DrawerConfig,
    compositor: &Rc<dyn dock_common::compositor::Compositor>,
) -> Option<gtk4::gdk::Monitor> {
    if config.output.is_empty() {
        return None;
    }
    let display = gtk4::gdk::Display::default()?;
    let monitors = display.monitors();
    let wm_monitors = compositor.list_monitors().ok()?;

    for (i, wm) in wm_monitors.iter().enumerate() {
        if wm.name == config.output
            && let Some(item) = monitors.item(i as u32)
            && let Ok(mon) = item.downcast::<gtk4::gdk::Monitor>()
        {
            return Some(mon);
        }
    }
    log::warn!("Target output '{}' not found", config.output);
    None
}

fn setup_close_button(main_vbox: &gtk4::Box, win: &gtk4::ApplicationWindow, config: &DrawerConfig) {
    use crate::config::CloseButton;

    let align = match config.closebtn {
        CloseButton::None => return,
        CloseButton::Left => gtk4::Align::Start,
        CloseButton::Right => gtk4::Align::End,
    };

    let close_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let close_btn = gtk4::Button::from_icon_name("window-close-symbolic");
    close_btn.add_css_class("flat");
    close_btn.set_widget_name("close-button");

    let win = win.clone();
    let resident = config.resident;
    close_btn.connect_clicked(move |_| {
        if !resident {
            win.close();
        } else {
            win.set_visible(false);
        }
    });

    close_box.set_halign(align);
    close_box.append(&close_btn);
    main_vbox.append(&close_box);
}
