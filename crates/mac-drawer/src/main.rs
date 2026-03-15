mod config;
mod desktop_loader;
mod state;
mod ui;
mod watcher;

use crate::config::DrawerConfig;
use crate::state::DrawerState;
use clap::Parser;
use dock_common::config::paths;
use dock_common::desktop::dirs::get_app_dirs;
use dock_common::pinning;
use dock_common::signals::{self, WindowCommand};
use dock_common::singleton;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Mac-style Launchpad CSS for the drawer.
const DRAWER_CSS: &str = r#"
window {
    background-color: rgba(22, 22, 30, 0.88);
    color: #e8e8e8;
}

/* Search entry — large, rounded, centered */
.drawer-search {
    font-size: 18px;
    padding: 12px 20px;
    margin: 20px 25%;
    border-radius: 12px;
    background-color: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    color: #ffffff;
    min-height: 24px;
}
.drawer-search:focus {
    border-color: rgba(100, 149, 237, 0.6);
    background-color: rgba(255, 255, 255, 0.12);
}

/* Section wells — rounded containers, content-width, centered */
.section-well {
    background-color: rgba(255, 255, 255, 0.04);
    border-radius: 16px;
    padding: 16px;
    margin: 8px auto;
    border: 1px solid rgba(255, 255, 255, 0.06);
}

/* App grid buttons */
.app-button {
    min-height: 0;
    min-width: 0;
    padding: 8px;
    border-radius: 12px;
    background: transparent;
    border: none;
}
.app-button:hover {
    background-color: rgba(255, 255, 255, 0.10);
}
.app-button image {
    margin: 0;
    padding: 0;
}
.app-button label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.85);
    margin-top: 4px;
}

/* Pinned section */
#pinned-box {
    padding-bottom: 8px;
}

/* Category buttons */
#category-button {
    margin: 4px 8px;
    padding: 6px 14px;
    border-radius: 8px;
    font-size: 13px;
    background-color: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.8);
}
#category-button:hover {
    background-color: rgba(255, 255, 255, 0.12);
}

/* File search results */
.file-result-well {
    background-color: rgba(255, 255, 255, 0.04);
    border-radius: 12px;
    padding: 12px;
    margin: 8px 40px;
    border: 1px solid rgba(255, 255, 255, 0.06);
}
.file-result-header {
    font-size: 13px;
    font-weight: bold;
    color: rgba(255, 255, 255, 0.6);
    padding: 4px 8px;
}
.file-result-item {
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 13px;
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.85);
}
.file-result-item:hover {
    background-color: rgba(255, 255, 255, 0.08);
}

/* Power bar */
.power-bar {
    margin-top: 12px;
    padding: 8px;
}

/* Status label */
.status-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.4);
    padding: 6px;
}

/* FlowBox children spacing */
flowboxchild {
    padding: 4px;
}
"#;

fn main() {
    let config = DrawerConfig::parse();

    if config.debug {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::init();
    }

    // Single instance check
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

    // Signal handler
    let sig_rx = Rc::new(signals::setup_signal_handlers(config.resident));

    // Paths
    let config_dir = paths::config_dir("nwg-drawer");
    paths::ensure_dir(&config_dir);

    let cache_dir = paths::cache_dir().expect("Couldn't determine cache directory");
    let pinned_file = cache_dir.join("mac-dock-pinned");

    // CSS path
    let css_path = if config.css_file.starts_with('/') {
        std::path::PathBuf::from(&config.css_file)
    } else {
        config_dir.join(&config.css_file)
    };

    // Copy default CSS if missing
    if !css_path.exists()
        && let Some(data_dir) = paths::find_data_home("nwg-drawer") {
            let src = data_dir.join("nwg-drawer/drawer.css");
            if let Err(e) = paths::copy_file(&src, &css_path) {
                log::warn!("Failed copying default CSS: {}", e);
            }
        }

    // App dirs
    let app_dirs = get_app_dirs();

    // Load exclusions
    let exclusions_file = config_dir.join("excluded-dirs");
    let exclusions = if exclusions_file.exists() {
        paths::load_text_lines(&exclusions_file).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Build GTK app
    let app = gtk4::Application::builder()
        .application_id("com.mac-drawer.hyprland")
        .build();

    let config = Rc::new(config);
    let pinned_file = Rc::new(pinned_file);
    let css_path = Rc::new(css_path);

    app.connect_activate(move |app| {
        let config = Rc::clone(&config);
        let pinned_file = Rc::clone(&pinned_file);
        let css_path = Rc::clone(&css_path);

        // Load CSS
        dock_common::config::css::load_css(&css_path);

        // Mac-style GTK4 overrides — polished Launchpad-like appearance
        dock_common::config::css::load_css_from_data(DRAWER_CSS);

        // Apply GTK theme/icon theme settings
        if let Some(settings) = gtk4::Settings::default() {
            if !config.gtk_theme.is_empty() {
                settings.set_gtk_theme_name(Some(&config.gtk_theme));
                log::info!("Using theme: {}", config.gtk_theme);
            } else {
                settings.set_property("gtk-application-prefer-dark-theme", true);
                log::info!("Preferring dark theme variants");
            }
            if !config.icon_theme.is_empty() {
                settings.set_gtk_icon_theme_name(Some(&config.icon_theme));
                log::info!("Using icon theme: {}", config.icon_theme);
            }
        }

        // Create state
        let state = Rc::new(RefCell::new(DrawerState::new(app_dirs.clone())));
        state.borrow_mut().exclusions = exclusions.clone();

        // Load desktop entries
        desktop_loader::load_desktop_entries(&mut state.borrow_mut());

        // Load preferred-apps.json for file associations
        let pa_file = paths::config_dir("nwg-drawer").join("preferred-apps.json");
        if pa_file.exists()
            && let Some(pa) = dock_common::desktop::preferred_apps::load_preferred_apps(&pa_file) {
                log::info!("Found {} custom file associations", pa.len());
                state.borrow_mut().preferred_apps = pa;
            }

        // Load pinned
        state.borrow_mut().pinned = pinning::load_pinned(&pinned_file);

        // Create window
        let win = gtk4::ApplicationWindow::new(app);

        // Monitor selection for -o flag
        let target_monitor = if !config.output.is_empty() {
            let display = gtk4::gdk::Display::default();
            let monitors = display.as_ref().map(|d| d.monitors());
            let mut found = None;
            if let (Some(monitors), Ok(hypr_monitors)) = (monitors, dock_common::hyprland::ipc::list_monitors()) {
                for (i, hm) in hypr_monitors.iter().enumerate() {
                    if hm.name == config.output
                        && let Some(item) = monitors.item(i as u32)
                            && let Ok(mon) = item.downcast::<gtk4::gdk::Monitor>() {
                                found = Some(mon);
                            }
                }
            }
            if found.is_none() {
                log::warn!("Target output '{}' not found", config.output);
            }
            found
        } else {
            None
        };
        ui::window::setup_drawer_window(&win, &config, target_monitor.as_ref());

        // Main layout
        let main_vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        win.set_child(Some(&main_vbox));

        // Close button (if configured)
        if config.closebtn != "none" {
            let close_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            let close_btn = gtk4::Button::from_icon_name("window-close-symbolic");
            close_btn.add_css_class("flat");
            close_btn.set_widget_name("close-button");
            let win_close = win.clone();
            let config_close = Rc::clone(&config);
            close_btn.connect_clicked(move |_| {
                if !config_close.resident {
                    win_close.close();
                } else {
                    win_close.set_visible(false);
                }
            });
            if config.closebtn == "left" {
                close_box.set_halign(gtk4::Align::Start);
            } else {
                close_box.set_halign(gtk4::Align::End);
            }
            close_box.append(&close_btn);
            main_vbox.append(&close_box);
        }

        // Search entry — large, centered, constrained width, with top padding
        let search_entry = ui::search::setup_search_entry();
        search_entry.add_css_class("drawer-search");
        search_entry.set_hexpand(false);
        search_entry.set_halign(gtk4::Align::Center);
        search_entry.set_width_request(500);
        search_entry.set_margin_top(40);
        main_vbox.append(&search_entry);

        // Scrolled window for content
        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);
        main_vbox.append(&scrolled);

        let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        scrolled.set_child(Some(&content_box));

        // Pinned section in a well
        let pinned_well = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        pinned_well.add_css_class("section-well");
        pinned_well.set_halign(gtk4::Align::Center);
        pinned_well.set_width_request(800);
        content_box.append(&pinned_well);

        // Status label at bottom
        let status_label = gtk4::Label::new(None);
        status_label.add_css_class("status-label");

        // On-launch callback (hide drawer if not resident)
        let win_launch = win.clone();
        let config_launch = Rc::clone(&config);
        let search_entry_launch = search_entry.clone();
        let on_launch: Rc<dyn Fn()> = Rc::new(move || {
            if !config_launch.resident {
                win_launch.close();
            } else {
                // Restore state and hide
                search_entry_launch.set_text("");
                win_launch.set_visible(false);
            }
        });

        // Build pinned flow box
        let pinned_flow = ui::pinned::build_pinned_flow_box(
            &config, &state, &pinned_file, Rc::clone(&on_launch),
        );
        pinned_well.append(&pinned_flow);
        // Hide pinned well if no items
        pinned_well.set_visible(!state.borrow().pinned.is_empty());

        // Categories + app grid in a well
        let app_well = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        app_well.add_css_class("section-well");
        app_well.set_halign(gtk4::Align::Center);
        // Set a reasonable width so the well doesn't collapse during search
        app_well.set_width_request(800);
        content_box.append(&app_well);

        if !config.no_cats {
            let categories_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            categories_box.set_halign(gtk4::Align::Center);
            app_well.append(&categories_box);

            let config_cat = Rc::clone(&config);
            let state_cat = Rc::clone(&state);
            let app_well_ref = app_well.clone();
            let on_launch_cat = Rc::clone(&on_launch);
            let pinned_file_cat = Rc::clone(&pinned_file);

            let app_flow_holder: Rc<RefCell<Option<gtk4::Box>>> = Rc::new(RefCell::new(None));
            let holder_ref = Rc::clone(&app_flow_holder);

            let on_cat = Rc::new(move |filter: Option<Vec<String>>| {
                if let Some(old) = holder_ref.borrow_mut().take() {
                    app_well_ref.remove(&old);
                }
                let wrapper = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                wrapper.set_hexpand(true);
                let flow = ui::app_grid::build_app_flow_box(
                    &config_cat,
                    &state_cat,
                    filter.as_deref(),
                    "",
                    &pinned_file_cat,
                    Rc::clone(&on_launch_cat),
                );
                flow.set_halign(gtk4::Align::Center);
                flow.set_hexpand(true);
                wrapper.append(&flow);
                app_well_ref.append(&wrapper);
                *holder_ref.borrow_mut() = Some(wrapper);
            });

            let cat_bar = ui::categories::build_category_bar(&state, on_cat.clone());
            categories_box.append(&cat_bar);

            // Initial app grid (all apps)
            on_cat(None);
        } else {
            // No categories — just show all apps in the well, centered
            let flow = ui::app_grid::build_app_flow_box(
                &config, &state, None, "", &pinned_file, Rc::clone(&on_launch),
            );
            flow.set_halign(gtk4::Align::Center);
            flow.set_hexpand(true);
            app_well.append(&flow);
        }

        // File search results section
        let file_search_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        file_search_box.set_visible(false);
        file_search_box.add_css_class("file-result-well");
        file_search_box.set_halign(gtk4::Align::Center);
        file_search_box.set_width_request(800);
        content_box.append(&file_search_box);

        // Search handler — on search, replace app_well contents with filtered results.
        // On clear, rebuild the full category view.
        let config_search = Rc::clone(&config);
        let state_search = Rc::clone(&state);
        let app_well_search = app_well.clone();
        let pinned_well_search = pinned_well.clone();
        let file_search_box_ref = file_search_box.clone();
        let on_launch_search = Rc::clone(&on_launch);
        let pinned_file_search = Rc::clone(&pinned_file);
        let status_label_search = status_label.clone();

        // Track the search-created app grid

        // Track whether we're in search mode (to know when to restore)
        let in_search_mode = Rc::new(RefCell::new(false));

        search_entry.connect_search_changed(move |entry| {
            let phrase = entry.text().to_string();

            if phrase.is_empty() {
                // Restore: show pinned, rebuild all apps in well
                if *in_search_mode.borrow() {
                    *in_search_mode.borrow_mut() = false;
                    // Clear search results from app well
                    while let Some(child) = app_well_search.first_child() {
                        app_well_search.remove(&child);
                    }
                    // Rebuild full app grid (all apps)
                    let flow = ui::app_grid::build_app_flow_box(
                        &config_search, &state_search, None, "",
                        &pinned_file_search, Rc::clone(&on_launch_search),
                    );
                    flow.set_halign(gtk4::Align::Center);
                    flow.set_hexpand(true);
                    app_well_search.append(&flow);

                    pinned_well_search.set_visible(true);
                }
                file_search_box_ref.set_visible(false);
                status_label_search.set_text("");
                return;
            }

            // Enter search mode — clear and replace app well contents
            if !*in_search_mode.borrow() {
                *in_search_mode.borrow_mut() = true;
                pinned_well_search.set_visible(false);
            }

            // Clear previous search results
            while let Some(child) = app_well_search.first_child() {
                app_well_search.remove(&child);
            }

            // Command mode (: prefix)
            if phrase.starts_with(':') {
                file_search_box_ref.set_visible(false);
                if phrase.len() > 1 {
                    let cmd_text = phrase.strip_prefix(':').unwrap_or(&phrase);
                    status_label_search.set_text(&format!("Execute \"{}\"", cmd_text));
                } else {
                    status_label_search.set_text("Execute a command");
                }
                return;
            }

            // App search — fill app_well with filtered results
            let flow = ui::app_grid::build_app_flow_box(
                &config_search, &state_search, None, &phrase,
                &pinned_file_search, Rc::clone(&on_launch_search),
            );
            flow.set_halign(gtk4::Align::Center);
            flow.set_hexpand(true);
            app_well_search.append(&flow);

            // File search (phrase > 2 chars)
            if !config_search.no_fs && phrase.len() > 2 {
                while let Some(child) = file_search_box_ref.first_child() {
                    file_search_box_ref.remove(&child);
                }
                let file_flow = ui::file_search::search_files(
                    &phrase, &config_search, &state_search,
                    Rc::clone(&on_launch_search),
                );
                file_search_box_ref.append(&file_flow);
                file_search_box_ref.set_visible(true);
            } else {
                file_search_box_ref.set_visible(false);
            }
        });

        // Power bar
        if config.has_power_bar() {
            let power_bar = ui::power_bar::build_power_bar(&config, Rc::clone(&on_launch));
            main_vbox.append(&power_bar);
        }

        // Status label at bottom of main layout
        main_vbox.append(&status_label);

        // Keyboard: Escape to clear/close, Enter for commands/math
        let win_key = win.clone();
        let config_key = Rc::clone(&config);
        let search_entry_key = search_entry.clone();
        let on_launch_key = Rc::clone(&on_launch);
        let app_ref = app.clone();
        let key_ctrl = gtk4::EventControllerKey::new();
        key_ctrl.connect_key_released(move |_, keyval, _, _| {
            match keyval {
                gtk4::gdk::Key::Escape => {
                    let text = search_entry_key.text();
                    if !text.is_empty() {
                        search_entry_key.set_text("");
                    } else if !config_key.resident {
                        win_key.close();
                    } else {
                        search_entry_key.set_text("");
                        win_key.set_visible(false);
                    }
                }
                gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => {
                    let text = search_entry_key.text().to_string();
                    if text.starts_with(':') && text.len() > 1 {
                        // Execute command via hyprctl dispatch
                        let cmd = &text[1..];
                        dock_common::launch::launch_hyprctl(cmd);
                        on_launch_key();
                    } else if let Some(result) = ui::math::eval_expression(&text) {
                        // Math expression
                        ui::math::show_result_window(&text, result, &app_ref);
                    }
                }
                _ => {}
            }
        });
        win.add_controller(key_ctrl);

        // Close drawer when another window gets focus (Hyprland IPC polling).
        // Records the active window at time of drawer show, only closes if a
        // DIFFERENT window gets focused afterwards.
        let win_focus = win.clone();
        let on_launch_focus = Rc::clone(&on_launch);
        let baseline_active: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
            if !win_focus.is_visible() {
                // Reset baseline when drawer is hidden
                *baseline_active.borrow_mut() = None;
                return glib::ControlFlow::Continue;
            }

            if let Ok(reply) = dock_common::hyprland::ipc::hyprctl("j/activewindow") {
                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&reply) {
                    let addr = val.get("address")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let class = val.get("class")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    if addr.is_empty() || class.is_empty() {
                        return glib::ControlFlow::Continue;
                    }

                    let mut baseline = baseline_active.borrow_mut();
                    if baseline.is_none() {
                        // First poll after drawer shown — record baseline
                        *baseline = Some(addr);
                    } else if baseline.as_deref() != Some(&addr) {
                        // Different window got focus — close drawer
                        *baseline = None;
                        drop(baseline);
                        on_launch_focus();
                    }
                }
            }
            glib::ControlFlow::Continue
        });

        win.present();

        // Start file watcher
        let watch_rx = watcher::start_watcher(&app_dirs, &pinned_file);
        let state_watch = Rc::clone(&state);
        let pinned_file_watch = Rc::clone(&pinned_file);
        let config_watch = Rc::clone(&config);
        let on_launch_watch = Rc::clone(&on_launch);
        let pinned_well_watch = pinned_well.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            while let Ok(event) = watch_rx.try_recv() {
                match event {
                    watcher::WatchEvent::DesktopFilesChanged => {
                        log::info!("Desktop files changed, reloading...");
                        desktop_loader::load_desktop_entries(&mut state_watch.borrow_mut());
                    }
                    watcher::WatchEvent::PinnedChanged => {
                        log::info!("Pinned file changed, rebuilding UI...");
                        state_watch.borrow_mut().pinned =
                            pinning::load_pinned(&pinned_file_watch);
                        // Rebuild pinned FlowBox visually
                        while let Some(child) = pinned_well_watch.first_child() {
                            pinned_well_watch.remove(&child);
                        }
                        let new_pinned = ui::pinned::build_pinned_flow_box(
                            &config_watch,
                            &state_watch,
                            &pinned_file_watch,
                            Rc::clone(&on_launch_watch),
                        );
                        pinned_well_watch.append(&new_pinned);
                        pinned_well_watch.set_visible(!state_watch.borrow().pinned.is_empty());
                    }
                }
            }
            glib::ControlFlow::Continue
        });

        // Signal handler polling
        let win_sig = win.clone();
        let sig_rx = Rc::clone(&sig_rx);
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            while let Ok(cmd) = sig_rx.try_recv() {
                match cmd {
                    WindowCommand::Show => win_sig.set_visible(true),
                    WindowCommand::Hide => win_sig.set_visible(false),
                    WindowCommand::Toggle => win_sig.set_visible(!win_sig.is_visible()),
                    WindowCommand::Quit => win_sig.close(),
                }
            }
            glib::ControlFlow::Continue
        });
    });

    app.run_with_args::<String>(&[]);
}
