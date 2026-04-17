use crate::config::DockConfig;
use crate::dock_windows::{self, MonitorDock};
use crate::monitor;
use crate::state::DockState;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use notify::{RecursiveMode, Watcher};
use nwg_dock_common::compositor::Compositor;
use nwg_dock_common::signals::WindowCommand;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

/// Delay before hiding dock windows after initial present (allows GTK to render).
const AUTOHIDE_INITIAL_DELAY: Duration = Duration::from_millis(500);

/// Interval for the dock liveness tick — detects missed monitor hotplug events,
/// zombie layer-shell surfaces (DPMS/lock cycles), and drift between expected
/// and actual dock windows. In-process pointer checks only in the cold path,
/// reconciliation only fires when state actually diverges.
const LIVENESS_TICK_INTERVAL: Duration = Duration::from_secs(2);

/// Sets up an inotify-based pin file watcher that triggers a rebuild
/// when the pin file is modified (e.g. by the drawer).
pub fn setup_pin_watcher(pinned_file: &Path, rebuild: &Rc<dyn Fn()>) {
    let pin_path = pinned_file.to_path_buf();
    let rebuild = Rc::clone(rebuild);
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let tx = tx;
        let mut watcher = match notify::recommended_watcher(move |res: Result<notify::Event, _>| {
            if let Ok(event) = res
                && matches!(
                    event.kind,
                    notify::EventKind::Modify(_) | notify::EventKind::Create(_)
                )
            {
                let _ = tx.send(()); // Non-critical: receiver may have dropped
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("Pin watcher failed: {}", e);
                return;
            }
        };

        if let Some(parent) = pin_path.parent()
            && let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive)
        {
            log::warn!(
                "Failed to watch pin file directory '{}': {}",
                parent.display(),
                e
            );
            return;
        }
        // Block forever — watcher stops if thread exits
        std::thread::park();
    });

    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        if rx.try_recv().is_ok() {
            while rx.try_recv().is_ok() {} // drain
            log::debug!("Pin file changed, rebuilding dock");
            rebuild();
        }
        glib::ControlFlow::Continue
    });
}

/// Sets up a signal handler poller that controls window visibility
/// based on SIGRTMIN+1/2/3 signals.
pub fn setup_signal_poller(
    app: &gtk4::Application,
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    sig_rx: &Rc<mpsc::Receiver<WindowCommand>>,
) {
    let app = app.clone();
    let docks = Rc::clone(per_monitor);
    let rx = Rc::clone(sig_rx);

    glib::timeout_add_local(Duration::from_millis(100), move || {
        while let Ok(cmd) = rx.try_recv() {
            // Quit shuts down the entire application (including hotspot windows)
            if matches!(cmd, WindowCommand::Quit) {
                app.quit();
                return glib::ControlFlow::Break;
            }
            let toggle_to = !docks.borrow().iter().any(|d| d.win.is_visible());
            for dock in docks.borrow().iter() {
                match cmd {
                    WindowCommand::Show => dock.win.set_visible(true),
                    WindowCommand::Hide => dock.win.set_visible(false),
                    WindowCommand::Toggle => dock.win.set_visible(toggle_to),
                    WindowCommand::Quit => unreachable!(),
                }
            }
        }
        glib::ControlFlow::Continue
    });
}

/// Sets up autohide: hides dock windows after initial show,
/// then starts the appropriate autohide mechanism for the compositor.
/// Returns a `HotspotContext` for Sway (used by reconciliation to create
/// hotspot windows for hotplugged monitors).
pub fn setup_autohide(
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &DockConfig,
    state: &Rc<RefCell<DockState>>,
    compositor: &Rc<dyn Compositor>,
    app: &gtk4::Application,
) -> Option<Rc<crate::ui::hotspot::HotspotContext>> {
    for dock in per_monitor.borrow().iter() {
        let win = dock.win.clone();
        glib::timeout_add_local_once(AUTOHIDE_INITIAL_DELAY, move || {
            win.set_visible(false);
        });
    }

    crate::ui::hotspot::setup_autohide(per_monitor, config, state, compositor, app)
}

/// Watches for GDK display monitor changes and reconciles dock windows.
///
/// Uses the `items-changed` signal on `Display::monitors()` to detect
/// monitor hotplug events. Debounced via idle callback to coalesce
/// rapid events (e.g., unplug + replug).
pub fn setup_monitor_watcher(
    app: &gtk4::Application,
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &Rc<DockConfig>,
    rebuild_fn: &Rc<dyn Fn()>,
    hotspot_ctx: Option<Rc<crate::ui::hotspot::HotspotContext>>,
) {
    let Some(display) = gtk4::gdk::Display::default() else {
        log::error!("No default GDK display for monitor watcher");
        return;
    };

    let model = display.monitors();
    let pending = Rc::new(Cell::new(false));
    let app = app.clone();
    let per_monitor = Rc::clone(per_monitor);
    let config = Rc::clone(config);
    let rebuild_fn = Rc::clone(rebuild_fn);

    model.connect_items_changed(move |_, _, _, _| {
        if pending.get() {
            return; // already scheduled
        }
        pending.set(true);

        let pending = Rc::clone(&pending);
        let app = app.clone();
        let per_monitor = Rc::clone(&per_monitor);
        let config = Rc::clone(&config);
        let rebuild_fn = Rc::clone(&rebuild_fn);
        let hotspot_ctx = hotspot_ctx.clone();

        glib::idle_add_local_once(move || {
            pending.set(false);
            log::info!("Monitor topology changed, reconciling dock windows");
            reconcile_monitors(
                &app,
                &per_monitor,
                &config,
                &rebuild_fn,
                hotspot_ctx.as_deref(),
            );
        });
    });
}

/// Reconciles dock windows with current monitor topology.
/// Creates windows for new monitors, destroys windows for removed monitors,
/// and rebuilds zombie windows (`surface().is_none()` — DPMS/lock cycles).
fn reconcile_monitors(
    app: &gtk4::Application,
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &DockConfig,
    rebuild_fn: &Rc<dyn Fn()>,
    hotspot_ctx: Option<&crate::ui::hotspot::HotspotContext>,
) {
    let current_monitors = monitor::resolve_monitors(config);
    let monitor_map: std::collections::HashMap<String, gtk4::gdk::Monitor> =
        current_monitors.into_iter().collect();
    let current_names: Vec<String> = monitor_map.keys().cloned().collect();
    let existing_names: Vec<String> = per_monitor
        .borrow()
        .iter()
        .map(|d| d.output_name.clone())
        .collect();

    let (mut to_add, mut to_remove) =
        dock_windows::compute_monitor_diff(&existing_names, &current_names);

    // Always refresh GDK monitor references — a reconnected monitor with the same
    // connector name produces a new gdk::Monitor object, and the old one is stale.
    refresh_monitor_refs(per_monitor, &monitor_map, hotspot_ctx);

    // Detect zombie windows: for monitors present in both sets, check if the
    // dock's layer-shell surface is still valid. If not, force remove+re-add.
    let zombies = find_zombie_docks(per_monitor, &current_names);
    for name in &zombies {
        log::warn!(
            "Zombie dock detected for '{}' (surface is None), rebuilding",
            name
        );
    }
    merge_zombies_into_diff(&mut to_add, &mut to_remove, &zombies);

    if to_add.is_empty() && to_remove.is_empty() {
        log::debug!("Monitor topology unchanged after debounce");
        return;
    }

    remove_orphaned_docks(per_monitor, &to_remove, hotspot_ctx);
    add_new_docks(app, per_monitor, &to_add, &monitor_map, config, hotspot_ctx);
    rebuild_fn();
}

/// Returns the output names of dock windows whose layer-shell surface is
/// missing (zombie state). A zombie window has an `ApplicationWindow` object
/// in our `per_monitor` Vec but `surface()` returns None — the compositor
/// has destroyed the underlying surface and the window is unrecoverable
/// without being re-created.
fn find_zombie_docks(
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    present_names: &[String],
) -> Vec<String> {
    let docks = per_monitor.borrow();
    let names: Vec<String> = docks.iter().map(|d| d.output_name.clone()).collect();
    let has_surface: Vec<bool> = docks.iter().map(|d| d.win.surface().is_some()).collect();
    drop(docks);
    zombie_names(&names, &has_surface, present_names)
}

/// Pure selection logic for `find_zombie_docks`. Given the dock list (names +
/// per-dock surface validity) and the names of monitors currently known by
/// GDK, returns the names of docks that are zombies — i.e. their monitor
/// still exists but their surface is gone.
fn zombie_names(
    dock_names: &[String],
    dock_has_surface: &[bool],
    present_names: &[String],
) -> Vec<String> {
    dock_names
        .iter()
        .zip(dock_has_surface.iter())
        .filter(|(name, has_surface)| !**has_surface && present_names.contains(name))
        .map(|(name, _)| name.clone())
        .collect()
}

/// Merges zombie names into the to_add/to_remove lists from `compute_monitor_diff`
/// so reconciliation destroys and re-creates each zombie's window. Dedupes
/// against existing entries so we don't remove or add a name twice.
fn merge_zombies_into_diff(
    to_add: &mut Vec<String>,
    to_remove: &mut Vec<String>,
    zombies: &[String],
) {
    for name in zombies {
        if !to_remove.contains(name) {
            to_remove.push(name.clone());
        }
        if !to_add.contains(name) {
            to_add.push(name.clone());
        }
    }
}

/// Starts a periodic liveness tick that catches missed hotplug events and
/// zombie layer-shell surfaces. Fires every `LIVENESS_TICK_INTERVAL`.
///
/// In the cold path (nothing wrong), each tick does only in-process pointer
/// checks — no IPC, no allocations of any real cost. Reconciliation is only
/// triggered when state actually diverges from expectations, so the hot path
/// runs about as often as real monitor state changes (rare).
pub fn setup_liveness_tick(
    app: &gtk4::Application,
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    config: &Rc<DockConfig>,
    rebuild_fn: &Rc<dyn Fn()>,
    hotspot_ctx: Option<Rc<crate::ui::hotspot::HotspotContext>>,
) {
    let app = app.clone();
    let per_monitor = Rc::clone(per_monitor);
    let config = Rc::clone(config);
    let rebuild_fn = Rc::clone(rebuild_fn);

    glib::timeout_add_local(LIVENESS_TICK_INTERVAL, move || {
        if needs_reconcile(&per_monitor, &config) {
            log::info!("Liveness tick detected state drift, reconciling");
            reconcile_monitors(
                &app,
                &per_monitor,
                &config,
                &rebuild_fn,
                hotspot_ctx.as_deref(),
            );
        }
        glib::ControlFlow::Continue
    });
}

/// Returns true if the dock's per-monitor state diverges from the monitor
/// set this config would select, or if any existing dock has a zombie surface.
/// Pure read-only checks — no IPC, no side effects.
///
/// Uses `monitor::resolve_monitors` (which honors `--output`) rather than the
/// raw GDK monitor list — otherwise a single-monitor-targeted dock would
/// perpetually "drift" against a multi-monitor GDK state.
fn needs_reconcile(per_monitor: &Rc<RefCell<Vec<MonitorDock>>>, config: &DockConfig) -> bool {
    let expected_names: Vec<String> = monitor::resolve_monitors(config)
        .into_iter()
        .map(|(name, _)| name)
        .collect();

    let docks = per_monitor.borrow();
    let dock_names: Vec<String> = docks.iter().map(|d| d.output_name.clone()).collect();
    let dock_has_surface: Vec<bool> = docks.iter().map(|d| d.win.surface().is_some()).collect();
    drop(docks);

    decide_reconcile(&expected_names, &dock_names, &dock_has_surface)
}

/// Pure decision logic for `needs_reconcile`. Testable without GTK —
/// given the current GDK monitor names and the dock's state (names +
/// per-dock surface validity), decides whether reconciliation is needed.
fn decide_reconcile(
    gdk_names: &[String],
    dock_names: &[String],
    dock_has_surface: &[bool],
) -> bool {
    // Cardinality guard: catches duplicate dock entries (same monitor mapped to
    // multiple windows) and parallel-array invariant violations. Simple
    // membership checks below miss the duplicate case when the extra entry
    // also has a GDK monitor match.
    if gdk_names.len() != dock_names.len() || dock_names.len() != dock_has_surface.len() {
        log::debug!(
            "Liveness: cardinality mismatch (gdk={}, docks={}, surfaces={})",
            gdk_names.len(),
            dock_names.len(),
            dock_has_surface.len()
        );
        return true;
    }

    // Missing monitor: GDK has a connector we don't have a dock for
    for name in gdk_names {
        if !dock_names.contains(name) {
            log::debug!("Liveness: missing dock for '{}'", name);
            return true;
        }
    }

    // Stale dock: we have a dock for a connector GDK doesn't report anymore
    for name in dock_names {
        if !gdk_names.contains(name) {
            log::debug!("Liveness: stale dock for '{}'", name);
            return true;
        }
    }

    // Zombie surface: dock object exists but layer-shell surface is gone
    for (name, has_surface) in dock_names.iter().zip(dock_has_surface.iter()) {
        if !has_surface {
            log::debug!("Liveness: zombie surface for '{}'", name);
            return true;
        }
    }

    false
}

/// Refreshes GDK monitor references on existing dock and hotspot windows.
fn refresh_monitor_refs(
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    monitor_map: &std::collections::HashMap<String, gtk4::gdk::Monitor>,
    hotspot_ctx: Option<&crate::ui::hotspot::HotspotContext>,
) {
    for dock in per_monitor.borrow().iter() {
        if let Some(mon) = monitor_map.get(&dock.output_name) {
            dock.win.set_monitor(Some(mon));
        }
    }
    if let Some(ctx) = hotspot_ctx {
        ctx.refresh_monitor_refs(monitor_map);
    }
}

/// Removes dock windows (and hotspot windows) for disconnected monitors.
fn remove_orphaned_docks(
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    to_remove: &[String],
    hotspot_ctx: Option<&crate::ui::hotspot::HotspotContext>,
) {
    for name in to_remove {
        if let Some(ctx) = hotspot_ctx {
            ctx.remove_hotspot_for_output(name);
        }
        per_monitor.borrow_mut().retain(|dock| {
            if &dock.output_name == name {
                log::info!("Removing dock window for disconnected monitor: {}", name);
                dock.win.close();
                false
            } else {
                true
            }
        });
    }
}

/// Creates dock windows (and hotspot windows) for newly connected monitors.
fn add_new_docks(
    app: &gtk4::Application,
    per_monitor: &Rc<RefCell<Vec<MonitorDock>>>,
    to_add: &[String],
    monitor_map: &std::collections::HashMap<String, gtk4::gdk::Monitor>,
    config: &DockConfig,
    hotspot_ctx: Option<&crate::ui::hotspot::HotspotContext>,
) {
    for name in to_add {
        let Some(gdk_mon) = monitor_map.get(name) else {
            continue;
        };
        log::info!("Creating dock window for new monitor: {}", name);
        let dock = dock_windows::create_single_dock_window(app, name, gdk_mon, config);
        dock.win.present();
        if config.autohide {
            let win = dock.win.clone();
            glib::timeout_add_local_once(AUTOHIDE_INITIAL_DELAY, move || {
                win.set_visible(false);
            });
        }
        if let Some(ctx) = hotspot_ctx {
            ctx.add_hotspot_for_dock(&dock);
        }
        per_monitor.borrow_mut().push(dock);
    }
}

#[cfg(test)]
mod tests {
    use super::{decide_reconcile, merge_zombies_into_diff, zombie_names};

    fn names(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_string()).collect()
    }

    // ─── decide_reconcile: steady-state and basic cases ────────────────────────

    #[test]
    fn decide_reconcile_steady_state_returns_false() {
        let gdk = names(&["DP-1", "HDMI-A-1"]);
        let docks = names(&["DP-1", "HDMI-A-1"]);
        let surfaces = vec![true, true];
        assert!(!decide_reconcile(&gdk, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_empty_state_stable() {
        // No monitors, no docks — nothing to reconcile
        assert!(!decide_reconcile(&[], &[], &[]));
    }

    #[test]
    fn decide_reconcile_single_monitor_stable() {
        let gdk = names(&["DP-1"]);
        let docks = names(&["DP-1"]);
        let surfaces = vec![true];
        assert!(!decide_reconcile(&gdk, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_three_monitors_all_healthy() {
        let gdk = names(&["DP-1", "DP-2", "HDMI-A-1"]);
        let docks = names(&["DP-1", "DP-2", "HDMI-A-1"]);
        let surfaces = vec![true, true, true];
        assert!(!decide_reconcile(&gdk, &docks, &surfaces));
    }

    // ─── decide_reconcile: missing dock (hotplug arrival) ──────────────────────

    #[test]
    fn decide_reconcile_detects_missing_dock_for_new_monitor() {
        let gdk = names(&["DP-1", "HDMI-A-1"]);
        let docks = names(&["DP-1"]);
        let surfaces = vec![true];
        assert!(decide_reconcile(&gdk, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_detects_all_monitors_missing_at_startup_race() {
        // Worst case: GDK knows monitors but we haven't built any docks yet.
        // Should still flag reconcile — otherwise we'd be permanently empty.
        let gdk = names(&["DP-1"]);
        assert!(decide_reconcile(&gdk, &[], &[]));
    }

    #[test]
    fn decide_reconcile_detects_third_monitor_missing() {
        let gdk = names(&["DP-1", "DP-2", "HDMI-A-1"]);
        let docks = names(&["DP-1", "DP-2"]);
        let surfaces = vec![true, true];
        assert!(decide_reconcile(&gdk, &docks, &surfaces));
    }

    // ─── decide_reconcile: stale dock (unplug missed) ──────────────────────────

    #[test]
    fn decide_reconcile_detects_stale_dock_after_unplug() {
        let gdk = names(&["DP-1"]);
        let docks = names(&["DP-1", "HDMI-A-1"]);
        let surfaces = vec![true, true];
        assert!(decide_reconcile(&gdk, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_detects_all_monitors_gone() {
        // Extreme case: every monitor unplugged but our docks linger
        let docks = names(&["DP-1", "HDMI-A-1"]);
        let surfaces = vec![true, true];
        assert!(decide_reconcile(&[], &docks, &surfaces));
    }

    // ─── decide_reconcile: zombie surfaces (DPMS/lock) ─────────────────────────

    #[test]
    fn decide_reconcile_detects_zombie_surface() {
        let gdk = names(&["DP-1"]);
        let docks = names(&["DP-1"]);
        let surfaces = vec![false];
        assert!(decide_reconcile(&gdk, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_detects_zombie_among_healthy() {
        let gdk = names(&["DP-1", "HDMI-A-1"]);
        let docks = names(&["DP-1", "HDMI-A-1"]);
        let surfaces = vec![true, false];
        assert!(decide_reconcile(&gdk, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_detects_all_zombies() {
        // Nightmare case: every dock went zombie (full DPMS wipe)
        let gdk = names(&["DP-1", "HDMI-A-1"]);
        let docks = names(&["DP-1", "HDMI-A-1"]);
        let surfaces = vec![false, false];
        assert!(decide_reconcile(&gdk, &docks, &surfaces));
    }

    // ─── decide_reconcile: combined divergences ────────────────────────────────

    #[test]
    fn decide_reconcile_missing_and_stale_together() {
        // DP-1 removed, HDMI-A-1 added — full swap
        let gdk = names(&["HDMI-A-1"]);
        let docks = names(&["DP-1"]);
        let surfaces = vec![true];
        assert!(decide_reconcile(&gdk, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_missing_and_zombie_together() {
        // A monitor was added, and an existing one's surface died
        let gdk = names(&["DP-1", "HDMI-A-1"]);
        let docks = names(&["DP-1"]);
        let surfaces = vec![false];
        assert!(decide_reconcile(&gdk, &docks, &surfaces));
    }

    // ─── decide_reconcile: ordering and duplicates ─────────────────────────────

    #[test]
    fn decide_reconcile_handles_reordered_names() {
        let gdk = names(&["HDMI-A-1", "DP-1"]);
        let docks = names(&["DP-1", "HDMI-A-1"]);
        let surfaces = vec![true, true];
        assert!(!decide_reconcile(&gdk, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_respects_config_output_filter() {
        // When --output DP-1 is set, the caller passes only the config-selected
        // monitor as `expected` — even though GDK has other monitors. A dock
        // matching that single expected monitor must not trigger drift.
        let expected = names(&["DP-1"]);
        let docks = names(&["DP-1"]);
        let surfaces = vec![true];
        assert!(!decide_reconcile(&expected, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_detects_duplicate_dock_for_same_monitor() {
        // Defensive: if reconciliation ever produced two docks for the same
        // monitor (startup race, double hotplug event, etc.), membership
        // checks alone wouldn't catch it. Cardinality guard triggers a heal.
        let gdk = names(&["DP-1"]);
        let docks = names(&["DP-1", "DP-1"]);
        let surfaces = vec![true, true];
        assert!(decide_reconcile(&gdk, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_detects_parallel_array_invariant_violation() {
        // Defensive: dock_names and dock_has_surface are parallel arrays
        // populated from the same Vec iteration. If they ever get out of
        // sync (caller bug), we must flag drift rather than silently
        // walking off the end via zip truncation.
        let gdk = names(&["DP-1"]);
        let docks = names(&["DP-1"]);
        let surfaces = vec![true, true]; // extra surface bool with no matching name
        assert!(decide_reconcile(&gdk, &docks, &surfaces));
    }

    #[test]
    fn decide_reconcile_idempotent_on_repeat_calls() {
        // Same input → same result, no hidden state
        let gdk = names(&["DP-1"]);
        let docks = names(&["DP-1"]);
        let surfaces = vec![true];
        let first = decide_reconcile(&gdk, &docks, &surfaces);
        let second = decide_reconcile(&gdk, &docks, &surfaces);
        assert_eq!(first, second);
        assert!(!first);
    }

    // ─── zombie_names: pure selection of broken docks ──────────────────────────

    #[test]
    fn zombie_names_empty_when_all_healthy() {
        let docks = names(&["DP-1", "HDMI-A-1"]);
        let surfaces = vec![true, true];
        let present = names(&["DP-1", "HDMI-A-1"]);
        assert!(zombie_names(&docks, &surfaces, &present).is_empty());
    }

    #[test]
    fn zombie_names_identifies_single_zombie() {
        let docks = names(&["DP-1", "HDMI-A-1"]);
        let surfaces = vec![true, false];
        let present = names(&["DP-1", "HDMI-A-1"]);
        assert_eq!(
            zombie_names(&docks, &surfaces, &present),
            names(&["HDMI-A-1"])
        );
    }

    #[test]
    fn zombie_names_identifies_multiple_zombies() {
        let docks = names(&["DP-1", "DP-2", "HDMI-A-1"]);
        let surfaces = vec![false, true, false];
        let present = names(&["DP-1", "DP-2", "HDMI-A-1"]);
        assert_eq!(
            zombie_names(&docks, &surfaces, &present),
            names(&["DP-1", "HDMI-A-1"])
        );
    }

    #[test]
    fn zombie_names_excludes_docks_for_removed_monitors() {
        // If a monitor is physically gone, its dock isn't a "zombie" — it's
        // stale and will be removed via the normal diff path. Don't double-count.
        let docks = names(&["DP-1", "HDMI-A-1"]);
        let surfaces = vec![true, false];
        let present = names(&["DP-1"]); // HDMI-A-1 is physically gone
        assert!(zombie_names(&docks, &surfaces, &present).is_empty());
    }

    #[test]
    fn zombie_names_empty_docks_produces_empty() {
        assert!(zombie_names(&[], &[], &names(&["DP-1"])).is_empty());
    }

    // ─── merge_zombies_into_diff: the bridge between diff and zombie handling ──

    #[test]
    fn merge_zombies_adds_to_both_lists_when_absent() {
        let mut to_add: Vec<String> = vec![];
        let mut to_remove: Vec<String> = vec![];
        let zombies = names(&["DP-1"]);
        merge_zombies_into_diff(&mut to_add, &mut to_remove, &zombies);
        assert_eq!(to_add, names(&["DP-1"]));
        assert_eq!(to_remove, names(&["DP-1"]));
    }

    #[test]
    fn merge_zombies_is_noop_when_already_queued() {
        // Zombie is also in to_add/to_remove from some other path — don't duplicate
        let mut to_add = names(&["DP-1"]);
        let mut to_remove = names(&["DP-1"]);
        let zombies = names(&["DP-1"]);
        merge_zombies_into_diff(&mut to_add, &mut to_remove, &zombies);
        assert_eq!(to_add, names(&["DP-1"]));
        assert_eq!(to_remove, names(&["DP-1"]));
    }

    #[test]
    fn merge_zombies_preserves_existing_entries() {
        // Existing hotplug-diff entries must not be clobbered
        let mut to_add = names(&["HDMI-A-1"]);
        let mut to_remove = names(&["DP-2"]);
        let zombies = names(&["DP-1"]);
        merge_zombies_into_diff(&mut to_add, &mut to_remove, &zombies);
        assert!(to_add.contains(&"HDMI-A-1".to_string()));
        assert!(to_add.contains(&"DP-1".to_string()));
        assert!(to_remove.contains(&"DP-2".to_string()));
        assert!(to_remove.contains(&"DP-1".to_string()));
    }

    #[test]
    fn merge_zombies_empty_list_is_noop() {
        let mut to_add = names(&["HDMI-A-1"]);
        let mut to_remove = names(&["DP-2"]);
        merge_zombies_into_diff(&mut to_add, &mut to_remove, &[]);
        assert_eq!(to_add, names(&["HDMI-A-1"]));
        assert_eq!(to_remove, names(&["DP-2"]));
    }

    #[test]
    fn merge_zombies_deduplicates_repeated_zombies_in_input() {
        // Defensive: zombies list shouldn't have duplicates in practice,
        // but if it does, merge should still produce a deduped result
        let mut to_add: Vec<String> = vec![];
        let mut to_remove: Vec<String> = vec![];
        let zombies = names(&["DP-1", "DP-1"]);
        merge_zombies_into_diff(&mut to_add, &mut to_remove, &zombies);
        assert_eq!(to_add, names(&["DP-1"]));
        assert_eq!(to_remove, names(&["DP-1"]));
    }

    #[test]
    fn merge_zombies_mixed_new_and_existing() {
        // One zombie is already in to_remove but not to_add — half-handled
        let mut to_add: Vec<String> = vec![];
        let mut to_remove = names(&["DP-1"]);
        let zombies = names(&["DP-1", "HDMI-A-1"]);
        merge_zombies_into_diff(&mut to_add, &mut to_remove, &zombies);
        assert_eq!(to_add, names(&["DP-1", "HDMI-A-1"]));
        assert_eq!(to_remove, names(&["DP-1", "HDMI-A-1"]));
    }
}
