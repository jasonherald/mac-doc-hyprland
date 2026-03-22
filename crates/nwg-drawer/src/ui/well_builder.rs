use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui;
use gtk4::prelude::*;
use nwg_dock_common::pinning;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Builds the normal (non-search) well content.
///
/// Pinned items go into `pinned_box` (above the ScrolledWindow, fixed).
/// App grid goes into `well` (inside the ScrolledWindow, scrollable).
pub fn build_normal_well(
    well: &gtk4::Box,
    pinned_box: &gtk4::Box,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) {
    clear_box(well);
    clear_box(pinned_box);

    let pinned = state.borrow().pinned.clone();

    // Rebuild callback shared by pinned unpin + app grid pin
    let on_rebuild = build_rebuild_callback(
        well,
        pinned_box,
        config,
        state,
        pinned_file,
        on_launch,
        status_label,
    );

    // Pinned items (above scroll)
    if !pinned.is_empty() {
        let pf = build_pinned_flow(
            config,
            state,
            pinned_file,
            on_launch,
            status_label,
            &on_rebuild,
        );
        pf.set_halign(gtk4::Align::Center);
        pinned_box.append(&pf);
    }

    // App grid (scrollable)
    let flow = ui::app_grid::build_app_flow_box(
        config,
        state,
        None,
        "",
        pinned_file,
        Rc::clone(on_launch),
        status_label,
        Some(&on_rebuild),
    );
    flow.set_halign(gtk4::Align::Center);
    well.append(&flow);

    // Install grid navigation on both FlowBoxes.
    let has_pinned = !pinned.is_empty();
    let pinned_flow_opt = if has_pinned {
        pinned_box
            .first_child()
            .and_then(|w| w.downcast::<gtk4::FlowBox>().ok())
    } else {
        None
    };

    // App grid gets capture-phase arrow handler + cross-section up
    install_grid_nav(
        &flow,
        config.columns,
        pinned_flow_opt.as_ref(), // up target
        None,                     // no down target (bottom of layout)
    );

    // Pinned grid gets capture-phase arrow handler + cross-section down
    if let Some(ref pf) = pinned_flow_opt {
        install_grid_nav(
            pf,
            config.columns.min(pinned.len() as u32).max(1),
            None,        // no up target (top of layout)
            Some(&flow), // down target
        );
    }
}

/// Builds search results — hides pinned, shows matching apps + files.
#[allow(clippy::too_many_arguments)]
pub fn build_search_results(
    well: &gtk4::Box,
    pinned_box: &gtk4::Box,
    phrase: &str,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) {
    clear_box(well);
    // Hide pinned during search
    pinned_box.set_visible(false);

    // Rebuild callback — rebuild_preserving_category checks active_search
    // and will re-run the search instead of restoring normal view.
    let on_rebuild = build_rebuild_callback(
        well,
        pinned_box,
        config,
        state,
        pinned_file,
        on_launch,
        status_label,
    );
    let app_flow = ui::app_grid::build_app_flow_box(
        config,
        state,
        None,
        phrase,
        pinned_file,
        Rc::clone(on_launch),
        status_label,
        Some(&on_rebuild),
    );
    app_flow.set_halign(gtk4::Align::Center);
    well.append(&app_flow);

    // Search results get navigation too (no cross-section targets)
    install_grid_nav(&app_flow, config.columns, None, None);

    // File results
    if !config.no_fs && phrase.len() > 2 {
        let file_results =
            ui::file_search::search_files(phrase, config, state, Rc::clone(on_launch));
        // file_search::search_files adds a header + separator before result rows
        let total_children = count_children(&file_results);
        let file_count = total_children.saturating_sub(2);
        if file_count > 0 {
            well.append(&divider());
            status_label.set_text(&format!(
                "{} file results | LMB: open | RMB: file manager",
                file_count
            ));
            file_results.set_halign(gtk4::Align::Center);
            well.append(&file_results);

            // Up from first file result → back to app search results
            install_file_results_nav(&file_results);
        }
    }
}

/// Rebuilds the well, preserving the current view mode (search, category, or normal).
#[allow(clippy::too_many_arguments)]
pub fn rebuild_preserving_category(
    well: &gtk4::Box,
    pinned_box: &gtk4::Box,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) {
    let active_search = state.borrow().active_search.clone();
    let active_cat = state.borrow().active_category.clone();

    match determine_rebuild_mode(&active_search, &active_cat) {
        RebuildMode::Search => {
            build_search_results(
                well,
                pinned_box,
                &active_search,
                config,
                state,
                pinned_file,
                on_launch,
                status_label,
            );
        }
        RebuildMode::Category => {
            build_normal_well(
                well,
                pinned_box,
                config,
                state,
                pinned_file,
                on_launch,
                status_label,
            );
            crate::ui::categories::apply_category_filter(
                well,
                pinned_box,
                config,
                state,
                &active_cat,
                pinned_file,
                on_launch,
                status_label,
            );
        }
        RebuildMode::Normal => {
            build_normal_well(
                well,
                pinned_box,
                config,
                state,
                pinned_file,
                on_launch,
                status_label,
            );
        }
    }
}

/// Restores the normal well (used when clearing search).
pub fn restore_normal_well(
    well: &gtk4::Box,
    pinned_box: &gtk4::Box,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) {
    pinned_box.set_visible(true);
    build_normal_well(
        well,
        pinned_box,
        config,
        state,
        pinned_file,
        on_launch,
        status_label,
    );
}

/// Builds the pinned items FlowBox with right-click unpin + immediate rebuild.
fn build_pinned_flow(
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
    on_rebuild: &Rc<dyn Fn()>,
) -> gtk4::FlowBox {
    let flow_box = gtk4::FlowBox::new();
    let pinned = state.borrow().pinned.clone();
    let cols = config.columns.min(pinned.len() as u32).max(1);
    flow_box.set_min_children_per_line(cols);
    flow_box.set_max_children_per_line(cols);
    flow_box.set_column_spacing(config.spacing);
    flow_box.set_row_spacing(config.spacing);
    flow_box.set_homogeneous(true);
    flow_box.set_selection_mode(gtk4::SelectionMode::None);

    let id2entry = state.borrow().apps.id2entry.clone();
    let app_dirs = state.borrow().app_dirs.clone();

    for desktop_id in &pinned {
        let entry = match id2entry.get(desktop_id) {
            Some(e) if !e.desktop_id.is_empty() && !e.no_display => e,
            _ => continue,
        };
        let button = build_pinned_button(
            entry,
            config,
            state,
            &app_dirs,
            pinned_file,
            on_launch,
            status_label,
            on_rebuild,
            desktop_id,
        );
        if config.pin_indicator {
            crate::ui::widgets::apply_pin_badge(&button);
        }
        flow_box.insert(&button, -1);
        // Keep FlowBoxChild non-focusable — we handle navigation ourselves
        if let Some(child) = flow_box.last_child() {
            child.set_focusable(false);
        }
    }

    flow_box
}

/// Builds a single pinned icon button with click-to-launch and right-click-to-unpin.
#[allow(clippy::too_many_arguments)]
fn build_pinned_button(
    entry: &nwg_dock_common::desktop::entry::DesktopEntry,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    app_dirs: &[std::path::PathBuf],
    pinned_file: &Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
    on_rebuild: &Rc<dyn Fn()>,
    desktop_id: &str,
) -> gtk4::Button {
    let name = if !entry.name_loc.is_empty() {
        &entry.name_loc
    } else {
        &entry.name
    };
    let desc = if !entry.comment_loc.is_empty() {
        &entry.comment_loc
    } else {
        &entry.comment
    };
    let button = crate::ui::widgets::app_icon_button(
        &entry.icon,
        name,
        config.icon_size,
        app_dirs,
        status_label,
        desc,
    );

    // Click → launch
    let exec = entry.exec.clone();
    let terminal = entry.terminal;
    let term = config.term.clone();
    let on_launch_ref = Rc::clone(on_launch);
    let compositor = Rc::clone(&state.borrow().compositor);
    let theme_prefix = state.borrow().gtk_theme_prefix.clone();
    button.connect_clicked(move |_| {
        let clean = crate::ui::widgets::clean_exec(&exec);
        if !clean.is_empty() {
            let cmd = crate::ui::widgets::prepend_theme(&clean, &theme_prefix);
            if terminal {
                nwg_dock_common::launch::launch_terminal_via_compositor(&cmd, &term, &*compositor);
            } else {
                nwg_dock_common::launch::launch_via_compositor(&cmd, &*compositor);
            }
            on_launch_ref();
        }
    });

    // Right-click → unpin + immediate rebuild
    let id = desktop_id.to_string();
    let state_ref = Rc::clone(state);
    let path = pinned_file.to_path_buf();
    let rebuild = Rc::clone(on_rebuild);
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let mut s = state_ref.borrow_mut();
        if pinning::unpin_item(&mut s.pinned, &id) {
            if let Err(e) = pinning::save_pinned(&s.pinned, &path) {
                log::error!("Failed to save pinned state: {}", e);
                // Restore the pin so UI stays in sync with disk
                s.pinned.push(id.clone());
                return;
            }
            log::info!("Unpinned {}", id);
            drop(s);
            rebuild();
        }
    });
    button.add_controller(gesture);

    button
}

/// Creates a callback that rebuilds the entire well + pinned_box.
/// Public so category filter can create rebuild callbacks for pin/unpin.
#[allow(clippy::too_many_arguments)]
pub fn build_rebuild_callback(
    well: &gtk4::Box,
    pinned_box: &gtk4::Box,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) -> Rc<dyn Fn()> {
    let well = well.clone();
    let pinned_box = pinned_box.clone();
    let config = config.clone();
    let state = Rc::clone(state);
    let pinned_file = pinned_file.to_path_buf();
    let on_launch = Rc::clone(on_launch);
    let status_label = status_label.clone();
    Rc::new(move || {
        let well = well.clone();
        let pinned_box = pinned_box.clone();
        let config = config.clone();
        let state = Rc::clone(&state);
        let pinned_file = pinned_file.clone();
        let on_launch = Rc::clone(&on_launch);
        let status_label = status_label.clone();
        gtk4::glib::idle_add_local_once(move || {
            rebuild_preserving_category(
                &well,
                &pinned_box,
                &config,
                &state,
                &pinned_file,
                &on_launch,
                &status_label,
            );
        });
    })
}

// ---------------------------------------------------------------------------
// Grid navigation — capture-phase controller that handles all arrow keys.
//
// GTK4's FlowBox internal `move_cursor` keybinding is unreliable with
// SelectionMode::None and non-focusable FlowBoxChildren (it consumes events
// without actually moving focus). We bypass it entirely by intercepting
// arrow keys in the Capture phase — before the FlowBox sees them.
// ---------------------------------------------------------------------------

/// Installs a capture-phase key controller on a FlowBox that handles
/// Up/Down/Left/Right arrow navigation within the grid and across sections.
///
/// `up_target` / `down_target`: optional FlowBox to jump to when reaching
/// the top/bottom edge of this grid.
pub fn install_grid_nav(
    flow: &gtk4::FlowBox,
    columns: u32,
    up_target: Option<&gtk4::FlowBox>,
    down_target: Option<&gtk4::FlowBox>,
) {
    let flow_ref = flow.clone();
    let up_ref = up_target.cloned();
    let down_ref = down_target.cloned();
    let cols = columns.max(1);

    // Remove any previous grid-nav controller to avoid stacking
    remove_named_controller(flow, "grid-nav");

    let ctrl = gtk4::EventControllerKey::new();
    ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    ctrl.set_name(Some("grid-nav"));

    ctrl.connect_key_pressed(move |_, keyval, _, _| {
        let total = count_flow_children(&flow_ref);
        if total == 0 {
            return gtk4::glib::Propagation::Proceed;
        }

        let (idx, col) = focused_position(&flow_ref, cols);

        match keyval {
            gtk4::gdk::Key::Right => nav_horizontal(&flow_ref, idx, 1, total),
            gtk4::gdk::Key::Left => nav_horizontal(&flow_ref, idx, -1, total),
            gtk4::gdk::Key::Down => nav_down(&flow_ref, idx, col, cols, total, &down_ref),
            gtk4::gdk::Key::Up => nav_up(&flow_ref, idx, col, cols, &up_ref),
            _ => gtk4::glib::Propagation::Proceed,
        }
    });

    flow.add_controller(ctrl);
}

/// Handles Left/Right navigation within a grid row.
fn nav_horizontal(
    flow: &gtk4::FlowBox,
    idx: i32,
    delta: i32,
    total: i32,
) -> gtk4::glib::Propagation {
    let next = idx + delta;
    if next >= 0 && next < total {
        focus_child_button(flow, next);
    }
    gtk4::glib::Propagation::Stop
}

/// Handles Down navigation: within grid, cross-section, or escape to next widget.
fn nav_down(
    flow: &gtk4::FlowBox,
    idx: i32,
    col: i32,
    cols: u32,
    total: i32,
    down_target: &Option<gtk4::FlowBox>,
) -> gtk4::glib::Propagation {
    let next = idx + cols as i32;
    if next < total {
        focus_child_button(flow, next);
        return gtk4::glib::Propagation::Stop;
    }
    // No item directly below — try cross-section FlowBox
    if let Some(target) = down_target {
        let target_total = count_flow_children(target);
        if target_total > 0 {
            focus_child_button(target, col.min(target_total - 1));
            return gtk4::glib::Propagation::Stop;
        }
        // Target exists but is empty — fall through to widget search
    }
    // No FlowBox target — try next visible widget (e.g. file results)
    if focus_next_visible(flow) {
        return gtk4::glib::Propagation::Stop;
    }
    gtk4::glib::Propagation::Stop
}

/// Handles Up navigation: within grid, cross-section, or escape to previous widget.
fn nav_up(
    flow: &gtk4::FlowBox,
    idx: i32,
    col: i32,
    cols: u32,
    up_target: &Option<gtk4::FlowBox>,
) -> gtk4::glib::Propagation {
    let prev = idx - cols as i32;
    if prev >= 0 {
        focus_child_button(flow, prev);
        return gtk4::glib::Propagation::Stop;
    }
    // Top edge — try cross-section FlowBox
    if let Some(target) = up_target {
        let target_total = count_flow_children(target);
        if target_total > 0 {
            let target_cols = target
                .max_children_per_line()
                .min(target_total as u32)
                .max(1);
            focus_child_button(
                target,
                find_column_from_bottom(col, target_cols, target_total),
            );
            return gtk4::glib::Propagation::Stop;
        }
        // Target exists but is empty — fall through to widget search
    }
    // No FlowBox target — focus nearest widget above (categories, search)
    if focus_prev_visible(flow) {
        return gtk4::glib::Propagation::Stop;
    }
    gtk4::glib::Propagation::Proceed
}

/// Walks up the widget tree from `start`, looking for the nearest visible
/// previous sibling that can accept focus. Handles nested containers like
/// ScrolledWindow by checking siblings at each ancestor level.
fn focus_prev_visible(start: &impl IsA<gtk4::Widget>) -> bool {
    let mut current = Some(start.as_ref().clone());
    while let Some(widget) = current {
        let mut prev = widget.prev_sibling();
        while let Some(p) = prev {
            if p.is_visible() && p.is_sensitive() && p.child_focus(gtk4::DirectionType::Up) {
                return true;
            }
            prev = p.prev_sibling();
        }
        current = widget.parent();
    }
    false
}

/// Walks down the widget tree from `start`, looking for the nearest visible
/// next sibling that can accept focus. Mirror of `focus_prev_visible`.
fn focus_next_visible(start: &impl IsA<gtk4::Widget>) -> bool {
    let mut current = Some(start.as_ref().clone());
    while let Some(widget) = current {
        let mut next = widget.next_sibling();
        while let Some(n) = next {
            if n.is_visible() && n.is_sensitive() && n.child_focus(gtk4::DirectionType::Down) {
                return true;
            }
            next = n.next_sibling();
        }
        current = widget.parent();
    }
    false
}

/// Finds the nearest item at `col` starting from the bottom row and walking up.
/// Handles partial last rows where the target column may not have an item.
fn find_column_from_bottom(col: i32, cols: u32, total: i32) -> i32 {
    let last_row = (total - 1) / cols as i32;
    for row in (0..=last_row).rev() {
        let idx = row * cols as i32 + col;
        if idx < total {
            return idx;
        }
    }
    total.saturating_sub(1)
}

/// Installs Up/Down navigation on file search results (vertical button list).
/// GTK handles Down between buttons natively. Up from the first button
/// needs to reach the app search FlowBox above.
fn install_file_results_nav(container: &gtk4::Box) {
    let container_ref = container.clone();
    let ctrl = gtk4::EventControllerKey::new();
    ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    ctrl.connect_key_pressed(move |_, keyval, _, _| {
        match keyval {
            gtk4::gdk::Key::Up => {
                // Check if focus is on the first button
                if let Some(first) = first_focusable_child(&container_ref)
                    && (first.has_focus() || first.is_focus())
                    && focus_prev_visible(&container_ref)
                {
                    return gtk4::glib::Propagation::Stop;
                }
                gtk4::glib::Propagation::Proceed
            }
            _ => gtk4::glib::Propagation::Proceed,
        }
    });
    container.add_controller(ctrl);
}

/// Returns the first focusable child widget (skipping headers/separators).
fn first_focusable_child(container: &gtk4::Box) -> Option<gtk4::Widget> {
    let mut child = container.first_child();
    while let Some(c) = child {
        if c.is_focusable() {
            return Some(c);
        }
        child = c.next_sibling();
    }
    None
}

/// Focuses the button inside the FlowBoxChild at the given index.
fn focus_child_button(flow: &gtk4::FlowBox, index: i32) {
    if let Some(child) = flow.child_at_index(index)
        && let Some(btn) = child.first_child()
    {
        btn.grab_focus();
    }
}

/// Returns the (index, column) of the currently focused child in a FlowBox.
/// Checks both the FlowBoxChild and its inner button for focus.
fn focused_position(flow: &gtk4::FlowBox, columns: u32) -> (i32, i32) {
    let mut idx = 0;
    let mut child = flow.first_child();
    while let Some(c) = child {
        if c.has_focus() || c.is_focus() {
            return (idx, idx % columns as i32);
        }
        if let Some(inner) = c.first_child()
            && (inner.has_focus() || inner.is_focus())
        {
            return (idx, idx % columns as i32);
        }
        idx += 1;
        child = c.next_sibling();
    }
    (0, 0)
}

fn count_flow_children(flow: &gtk4::FlowBox) -> i32 {
    let mut n = 0;
    let mut child = flow.first_child();
    while let Some(c) = child {
        n += 1;
        child = c.next_sibling();
    }
    n
}

/// Removes an event controller by name from a widget.
fn remove_named_controller(widget: &impl IsA<gtk4::Widget>, name: &str) {
    let mut controllers = Vec::new();
    let list = widget.observe_controllers();
    for i in 0..list.n_items() {
        if let Some(obj) = list.item(i)
            && let Ok(ctrl) = obj.downcast::<gtk4::EventController>()
            && ctrl.name().as_deref() == Some(name)
        {
            controllers.push(ctrl);
        }
    }
    for ctrl in controllers {
        widget.remove_controller(&ctrl);
    }
}

fn clear_box(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn divider() -> gtk4::Separator {
    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    sep.set_margin_top(8);
    sep.set_margin_bottom(8);
    sep.set_margin_start(16);
    sep.set_margin_end(16);
    sep
}

fn count_children(widget: &impl IsA<gtk4::Widget>) -> i32 {
    let mut count = 0;
    let mut child = widget.first_child();
    while let Some(c) = child {
        count += 1;
        child = c.next_sibling();
    }
    count
}

/// Which rebuild path to take when refreshing the well.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RebuildMode {
    /// Re-run the active search query.
    Search,
    /// Rebuild normal well then re-apply category filter.
    Category,
    /// Rebuild normal well (show all apps).
    Normal,
}

/// Pure decision function: determines the rebuild mode from current state.
/// Search takes precedence over category (you can search within a category view).
fn determine_rebuild_mode(active_search: &str, active_category: &[String]) -> RebuildMode {
    if !active_search.is_empty() {
        RebuildMode::Search
    } else if !active_category.is_empty() {
        RebuildMode::Category
    } else {
        RebuildMode::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_mode_search_takes_precedence() {
        assert_eq!(
            determine_rebuild_mode("firefox", &["Network".to_string()]),
            RebuildMode::Search
        );
    }

    #[test]
    fn rebuild_mode_category_when_no_search() {
        assert_eq!(
            determine_rebuild_mode("", &["Network".to_string()]),
            RebuildMode::Category
        );
    }

    #[test]
    fn rebuild_mode_normal_when_both_empty() {
        assert_eq!(determine_rebuild_mode("", &[]), RebuildMode::Normal);
    }
}
