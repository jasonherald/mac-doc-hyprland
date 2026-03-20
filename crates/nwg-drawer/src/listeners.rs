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

/// Sets up keyboard handler for the drawer window.
///
/// - Navigation keys (arrows, Tab, Page Up/Down, Home/End) propagate to
///   FlowBox children for keyboard navigation between app icons
/// - Escape clears search or closes the drawer
/// - Return handles `:command` execution and math evaluation (only when
///   the search entry has focus — otherwise it propagates to the focused
///   button, launching the app via GTK4's button activate)
/// - Any other key auto-focuses the search entry so typing starts a search
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

    // Key press handler — intercepts Escape, Return, and auto-focus-search.
    // Capture phase so it fires even when no widget has focus (e.g. fresh open).
    // Navigation keys return Proceed so GTK handles focus movement.
    let key_ctrl = gtk4::EventControllerKey::new();
    key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
        match keyval {
            gtk4::gdk::Key::Escape => {
                handle_escape(&search_entry, &win, config.resident);
                gtk4::glib::Propagation::Stop
            }

            gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => {
                handle_return(&search_entry, &*compositor, &on_launch, &app);
                gtk4::glib::Propagation::Proceed
            }

            // Navigation keys — let GTK handle focus movement
            gtk4::gdk::Key::Up
            | gtk4::gdk::Key::Down
            | gtk4::gdk::Key::Left
            | gtk4::gdk::Key::Right
            | gtk4::gdk::Key::Tab
            | gtk4::gdk::Key::ISO_Left_Tab
            | gtk4::gdk::Key::Page_Up
            | gtk4::gdk::Key::Page_Down
            | gtk4::gdk::Key::Home
            | gtk4::gdk::Key::End => gtk4::glib::Propagation::Proceed,

            // Any other key — auto-focus search entry so typing starts a search
            _ => {
                if !search_entry.has_focus() {
                    search_entry.grab_focus();
                }
                gtk4::glib::Propagation::Proceed
            }
        }
    });
    win_ctrl.add_controller(key_ctrl);
}

/// Polls compositor active window to close drawer when another window gets focus.
pub fn setup_focus_detector(
    win: &gtk4::ApplicationWindow,
    on_launch: &Rc<dyn Fn()>,
    compositor: &Rc<dyn Compositor>,
) {
    // Close immediately when the GTK window loses focus (user clicked elsewhere,
    // including empty desktop on another monitor). Works cross-compositor.
    {
        let on_launch = Rc::clone(on_launch);
        let win_ref = win.clone();
        win.connect_is_active_notify(move |_| {
            if !win_ref.is_active() {
                on_launch();
            }
        });
    }

    let win = win.clone();
    let on_launch = Rc::clone(on_launch);
    let compositor = Rc::clone(compositor);
    let baseline: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
        if !win.is_visible() {
            *baseline.borrow_mut() = None;
            return glib::ControlFlow::Continue;
        }
        poll_active_window(&compositor, &baseline, &on_launch);
        glib::ControlFlow::Continue
    });
}

/// Sets up inotify-based file watcher for pin and desktop file changes.
#[allow(clippy::too_many_arguments)]
pub fn setup_file_watcher(
    app_dirs: &[std::path::PathBuf],
    pinned_file: &Rc<PathBuf>,
    well: &gtk4::Box,
    pinned_box: &gtk4::Box,
    config: &Rc<DrawerConfig>,
    state: &Rc<RefCell<DrawerState>>,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) {
    let watch_rx = watcher::start_watcher(app_dirs, pinned_file);
    let state = Rc::clone(state);
    let pinned_file = Rc::clone(pinned_file);
    let well = well.clone();
    let pinned_box = pinned_box.clone();
    let config = Rc::clone(config);
    let on_launch = Rc::clone(on_launch);
    let status_label = status_label.clone();

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        while let Ok(event) = watch_rx.try_recv() {
            match event {
                watcher::WatchEvent::DesktopFilesChanged => {
                    log::info!("Desktop files changed, reloading...");
                    desktop_loader::load_desktop_entries(&mut state.borrow_mut());
                    well_builder::rebuild_preserving_category(
                        &well,
                        &pinned_box,
                        &config,
                        &state,
                        &pinned_file,
                        &on_launch,
                        &status_label,
                    );
                }
                watcher::WatchEvent::PinnedChanged => {
                    log::info!("Pinned file changed, rebuilding...");
                    state.borrow_mut().pinned = pinning::load_pinned(&pinned_file);
                    well_builder::rebuild_preserving_category(
                        &well,
                        &pinned_box,
                        &config,
                        &state,
                        &pinned_file,
                        &on_launch,
                        &status_label,
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

/// Checks if the active window changed and closes the drawer if so.
fn poll_active_window(
    compositor: &Rc<dyn Compositor>,
    baseline: &Rc<RefCell<Option<String>>>,
    on_launch: &Rc<dyn Fn()>,
) {
    let active = match compositor.get_active_window() {
        Ok(a) => a,
        Err(_) => {
            // Compositor error (e.g. workspace with no windows) — close
            close_if_baseline_set(baseline, on_launch);
            return;
        }
    };

    // Empty id+class means no window focused (e.g. switched workspace) — close
    if active.id.is_empty() && active.class.is_empty() {
        close_if_baseline_set(baseline, on_launch);
        return;
    }

    // Skip partial responses (e.g. layer-shell surfaces)
    if active.id.is_empty() || active.class.is_empty() {
        return;
    }

    let mut b = baseline.borrow_mut();
    if b.is_none() {
        *b = Some(active.id);
    } else if b.as_deref() != Some(&active.id) {
        *b = None;
        drop(b);
        on_launch();
    }
}

/// Clears baseline and fires on_launch if a baseline was set.
fn close_if_baseline_set(baseline: &Rc<RefCell<Option<String>>>, on_launch: &Rc<dyn Fn()>) {
    let mut b = baseline.borrow_mut();
    if b.is_some() {
        *b = None;
        drop(b);
        on_launch();
    }
}

/// Handles Escape key: clear search, or close/hide drawer.
fn handle_escape(search_entry: &gtk4::SearchEntry, win: &gtk4::ApplicationWindow, resident: bool) {
    let text = search_entry.text();
    if !text.is_empty() {
        search_entry.set_text("");
    } else if !resident {
        win.close();
    } else {
        search_entry.set_text("");
        win.set_visible(false);
    }
}

/// Handles Return key: execute `:command` or evaluate math (only when search has focus).
fn handle_return(
    search_entry: &gtk4::SearchEntry,
    compositor: &dyn Compositor,
    on_launch: &Rc<dyn Fn()>,
    app: &gtk4::Application,
) {
    if !search_entry.has_focus() {
        return;
    }
    let text = search_entry.text().to_string();
    if text.starts_with(':') && text.len() > 1 {
        let cmd = &text[1..];
        nwg_dock_common::launch::launch_via_compositor(cmd, compositor);
        on_launch();
    } else if let Some(result) = crate::ui::math::eval_expression(&text) {
        crate::ui::math::show_result_window(&text, result, app);
    }
}
