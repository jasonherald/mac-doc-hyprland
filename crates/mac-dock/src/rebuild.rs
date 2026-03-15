use crate::config::DockConfig;
use crate::dock_windows::MonitorDock;
use crate::state::DockState;
use crate::ui;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Creates the rebuild function that rebuilds dock content on all monitors.
///
/// Uses `Rc::new_cyclic` to allow the rebuild function to reference itself
/// (needed so buttons inside the dock can trigger a rebuild on pin/unpin).
pub fn create_rebuild_fn(
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &Rc<DockConfig>,
    state: &Rc<RefCell<DockState>>,
    data_home: &Rc<std::path::PathBuf>,
    pinned_file: &Rc<std::path::PathBuf>,
) -> Rc<dyn Fn()> {
    let per_monitor = Rc::clone(per_monitor);
    let config = Rc::clone(config);
    let state = Rc::clone(state);
    let data_home = Rc::clone(data_home);
    let pinned_file = Rc::clone(pinned_file);

    // Use a holder so the rebuild fn can pass itself to button constructors
    type RebuildHolder = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
    let holder: RebuildHolder = Rc::new(RefCell::new(None));

    let rebuild_fn = {
        let holder = Rc::clone(&holder);
        Rc::new(move || {
            let self_ref = holder
                .borrow()
                .clone()
                .unwrap_or_else(|| Rc::new(|| {}));

            for dock in per_monitor.borrow().iter() {
                if let Some(old) = dock.current_main_box.borrow_mut().take() {
                    dock.alignment_box.remove(&old);
                }
                let new_box = ui::dock_box::build(
                    &dock.alignment_box,
                    &config,
                    &state,
                    &data_home,
                    &pinned_file,
                    &self_ref,
                    &dock.win,
                );
                *dock.current_main_box.borrow_mut() = Some(new_box);
            }
        })
    };

    *holder.borrow_mut() = Some(rebuild_fn.clone() as Rc<dyn Fn()>);
    rebuild_fn
}
