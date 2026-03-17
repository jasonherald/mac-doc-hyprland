use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::well_builder;
use crate::{desktop_loader, watcher};
use gtk4::glib;
use gtk4::prelude::*;
use nwg_dock_common::compositor::Compositor;
use nwg_dock_common::pinning;
use nwg_dock_common::signals::WindowCommand;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;

/// Sets up keyboard handler for Escape (close/clear) and Enter (command/math).
pub fn setup_keyboard(
    win: &gtk4::ApplicationWindow,
    search_entry: &gtk4::SearchEntry,
    config: &Rc<DrawerConfig>,
    on_launch: &Rc<dyn Fn()>,
    app: &gtk4::Application,
    compositor: &Rc<dyn Compositor>,
) {
    let win_ctrl = win.clone();
    let win = win.clone();
    let config = Rc::clone(config);
    let search_entry = search_entry.clone();
    let on_launch = Rc::clone(on_launch);
    let app = app.clone();
    let compositor = Rc::clone(compositor);

    let key_ctrl = gtk4::EventControllerKey::new();
    key_ctrl.connect_key_released(move |_, keyval, _, _| match keyval {
        gtk4::gdk::Key::Escape => {
            let text = search_entry.text();
            if !text.is_empty() {
                search_entry.set_text("");
            } else if !config.resident {
                win.close();
            } else {
                search_entry.set_text("");
                win.set_visible(false);
            }
        }
        gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => {
            let text = search_entry.text().to_string();
            if text.starts_with(':') && text.len() > 1 {
                let cmd = &text[1..];
                nwg_dock_common::launch::launch_via_compositor(cmd, &*compositor);
                on_launch();
            } else if let Some(result) = crate::ui::math::eval_expression(&text) {
                crate::ui::math::show_result_window(&text, result, &app);
            }
        }
        _ => {}
    });
    win_ctrl.add_controller(key_ctrl);
}

/// Polls compositor active window to close drawer when another window gets focus.
pub fn setup_focus_detector(
    win: &gtk4::ApplicationWindow,
    on_launch: &Rc<dyn Fn()>,
    compositor: &Rc<dyn Compositor>,
) {
    let win = win.clone();
    let on_launch = Rc::clone(on_launch);
    let compositor = Rc::clone(compositor);
    let baseline: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
        if !win.is_visible() {
            *baseline.borrow_mut() = None;
            return glib::ControlFlow::Continue;
        }

        if let Ok(active) = compositor.get_active_window() {
            let id = active.id;
            let class = active.class;

            if id.is_empty() || class.is_empty() {
                return glib::ControlFlow::Continue;
            }

            let mut b = baseline.borrow_mut();
            if b.is_none() {
                *b = Some(id);
            } else if b.as_deref() != Some(&id) {
                *b = None;
                drop(b);
                on_launch();
            }
        }

        glib::ControlFlow::Continue
    });
}

/// Sets up inotify-based file watcher for pin and desktop file changes.
pub fn setup_file_watcher(
    app_dirs: &[std::path::PathBuf],
    pinned_file: &Rc<PathBuf>,
    well: &gtk4::Box,
    config: &Rc<DrawerConfig>,
    state: &Rc<RefCell<DrawerState>>,
    on_launch: &Rc<dyn Fn()>,
) {
    let watch_rx = watcher::start_watcher(app_dirs, pinned_file);
    let state = Rc::clone(state);
    let pinned_file = Rc::clone(pinned_file);
    let well = well.clone();
    let config = Rc::clone(config);
    let on_launch = Rc::clone(on_launch);

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        while let Ok(event) = watch_rx.try_recv() {
            match event {
                watcher::WatchEvent::DesktopFilesChanged => {
                    log::info!("Desktop files changed, reloading...");
                    desktop_loader::load_desktop_entries(&mut state.borrow_mut());
                    well_builder::build_normal_well(
                        &well,
                        &config,
                        &state,
                        &pinned_file,
                        &on_launch,
                    );
                }
                watcher::WatchEvent::PinnedChanged => {
                    log::info!("Pinned file changed, rebuilding...");
                    state.borrow_mut().pinned = pinning::load_pinned(&pinned_file);
                    well_builder::build_normal_well(
                        &well,
                        &config,
                        &state,
                        &pinned_file,
                        &on_launch,
                    );
                }
            }
        }
        glib::ControlFlow::Continue
    });
}

/// Sets up signal handler polling for SIGRTMIN+1/2/3.
pub fn setup_signal_poller(
    win: &gtk4::ApplicationWindow,
    sig_rx: &Rc<mpsc::Receiver<WindowCommand>>,
) {
    let win = win.clone();
    let rx = Rc::clone(sig_rx);

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                WindowCommand::Show => win.set_visible(true),
                WindowCommand::Hide => win.set_visible(false),
                WindowCommand::Toggle => win.set_visible(!win.is_visible()),
                WindowCommand::Quit => win.close(),
            }
        }
        glib::ControlFlow::Continue
    });
}
