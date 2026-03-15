use crate::config::DockConfig;
use crate::ui;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use std::cell::RefCell;
use std::rc::Rc;

/// Per-monitor dock window state used during rebuilds.
pub struct MonitorDock {
    pub alignment_box: gtk4::Box,
    pub current_main_box: Rc<RefCell<Option<gtk4::Box>>>,
    pub win: gtk4::ApplicationWindow,
}

/// Creates a dock window for each monitor and returns the per-monitor state.
pub fn create_dock_windows(
    app: &gtk4::Application,
    monitors: &[gtk4::gdk::Monitor],
    config: &DockConfig,
) -> (Vec<MonitorDock>, Rc<RefCell<Vec<gtk4::ApplicationWindow>>>) {
    let mut per_monitor = Vec::new();
    let all_windows: Rc<RefCell<Vec<gtk4::ApplicationWindow>>> =
        Rc::new(RefCell::new(Vec::new()));

    for mon in monitors {
        let win = gtk4::ApplicationWindow::new(app);
        ui::window::setup_dock_window(&win, config);
        win.set_monitor(Some(mon));

        let (outer_orient, _) = ui::window::orientations(config);
        let outer_box = gtk4::Box::new(outer_orient, 0);
        outer_box.set_widget_name("box");
        win.set_child(Some(&outer_box));

        let inner_orient = if config.is_vertical() {
            gtk4::Orientation::Vertical
        } else {
            gtk4::Orientation::Horizontal
        };
        let alignment_box = gtk4::Box::new(inner_orient, 0);
        if config.full {
            alignment_box.set_hexpand(true);
            alignment_box.set_vexpand(true);
        }
        outer_box.append(&alignment_box);

        per_monitor.push(MonitorDock {
            alignment_box,
            current_main_box: Rc::new(RefCell::new(None)),
            win: win.clone(),
        });

        all_windows.borrow_mut().push(win);
    }

    (per_monitor, all_windows)
}
