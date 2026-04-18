use crate::config::DockConfig;
use crate::context::DockContext;
use crate::dock_windows::MonitorDock;
use crate::state::DockState;
use crate::ui;
use gtk4::prelude::*;
use nwg_dock_common::compositor::Compositor;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

/// Creates the rebuild function that rebuilds dock content on all monitors.
///
/// Uses `Weak` for the self-reference to avoid an Rc cycle. Buttons inside
/// the dock can trigger a rebuild via the `DockContext.rebuild` callback.
///
/// Reentrancy is guarded: `dock_box::build()` calls into glycin for icon
/// loading, which uses D-Bus and pumps the GTK main loop. That can let
/// another timer/event fire and call rebuild_fn while we're mid-build,
/// which previously left ghost widgets in `alignment_box`. The guard
/// turns recursive calls into a "pending" flag and re-runs once the
/// current rebuild finishes.
pub fn create_rebuild_fn(
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &Rc<DockConfig>,
    state: &Rc<RefCell<DockState>>,
    data_home: &Rc<std::path::PathBuf>,
    pinned_file: &Rc<std::path::PathBuf>,
    compositor: &Rc<dyn Compositor>,
) -> Rc<dyn Fn()> {
    let per_monitor = Rc::clone(per_monitor);
    let config = Rc::clone(config);
    let state = Rc::clone(state);
    let data_home = Rc::clone(data_home);
    let pinned_file = Rc::clone(pinned_file);
    let compositor = Rc::clone(compositor);

    // Use Weak to break the Rc cycle: rebuild_fn → holder → rebuild_fn
    type RebuildHolder = Rc<RefCell<Weak<dyn Fn()>>>;
    let holder: RebuildHolder = Rc::new(RefCell::new(Weak::<Box<dyn Fn()>>::new()));

    // Reentrancy guards. `running` is set while a rebuild is in flight.
    // `pending` is set if rebuild_fn is called while one is already running,
    // and triggers a re-run once the current rebuild completes.
    let running = Rc::new(Cell::new(false));
    let pending = Rc::new(Cell::new(false));

    let rebuild_fn = {
        let holder = Rc::clone(&holder);
        let running = Rc::clone(&running);
        let pending = Rc::clone(&pending);

        Rc::new(move || {
            if running.get() {
                // Mid-flight rebuild detected (likely glycin pumping the
                // main loop). Don't recurse — flag the request and the
                // outer loop below will pick it up.
                pending.set(true);
                return;
            }

            running.set(true);

            loop {
                pending.set(false);

                // Upgrade the weak self-reference for passing to buttons
                let rebuild_ref: Rc<dyn Fn()> =
                    holder.borrow().upgrade().unwrap_or_else(|| Rc::new(|| {}));

                let ctx = DockContext {
                    config: Rc::clone(&config),
                    state: Rc::clone(&state),
                    data_home: Rc::clone(&data_home),
                    pinned_file: Rc::clone(&pinned_file),
                    rebuild: rebuild_ref,
                    compositor: Rc::clone(&compositor),
                };

                for dock in per_monitor.borrow().iter() {
                    // Defense-in-depth: remove ALL children from
                    // alignment_box, not just the tracked main_box. If a
                    // ghost widget ever slipped through (older bug or
                    // future regression), this purges it.
                    while let Some(child) = dock.alignment_box.first_child() {
                        dock.alignment_box.remove(&child);
                    }
                    dock.current_main_box.borrow_mut().take();

                    let new_box = ui::dock_box::build(&dock.alignment_box, &ctx, &dock.win);
                    let new_count = count_children(&new_box);
                    *dock.current_main_box.borrow_mut() = Some(new_box);

                    // Layer-shell surfaces don't shrink on their own when
                    // content shrinks — GTK keeps a high-water-mark
                    // allocation and `queue_resize` / `present` aren't enough
                    // to invalidate it. The only reliable way is a hide/show
                    // cycle, which tears the surface down and re-creates it
                    // at the new natural size.
                    //
                    // Doing that on every rebuild causes a visible flicker,
                    // so we only force the cycle when the item count actually
                    // dropped — i.e. when the surface is now larger than it
                    // needs to be (issue #62). Growing or steady-state
                    // rebuilds don't need it; the surface naturally accepts a
                    // larger natural-size request. Autohide users never see
                    // this bug because the hover-driven hide/show does the
                    // same thing for free.
                    let prev = dock.prev_item_count.get();
                    if dock.win.is_visible() && new_count < prev {
                        let win = dock.win.clone();
                        gtk4::glib::idle_add_local_once(move || {
                            win.set_visible(false);
                            win.set_visible(true);
                        });
                    }
                    dock.prev_item_count.set(new_count);
                }

                // If another rebuild was requested during this iteration
                // (glycin pumped the loop, a timer fired, etc.), do it
                // again with the latest state. Otherwise we're done.
                if !pending.get() {
                    break;
                }
            }

            running.set(false);
        })
    };

    // Store a Weak reference — no cycle
    *holder.borrow_mut() = Rc::downgrade(&rebuild_fn) as Weak<dyn Fn()>;
    rebuild_fn
}

/// Counts the immediate children of a Box. Used to compare new vs previous
/// rebuild item counts so we can detect content shrinkage and trigger the
/// layer-shell surface reset only when actually needed (see issue #62 fix
/// in the rebuild loop above).
fn count_children(parent: &gtk4::Box) -> usize {
    let mut n = 0;
    let mut child = parent.first_child();
    while let Some(w) = child {
        n += 1;
        child = w.next_sibling();
    }
    n
}
