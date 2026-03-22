use gtk4::prelude::*;

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

/// Installs Up/Down navigation on file search results (vertical button list).
/// GTK handles Down between buttons natively. Up from the first button
/// needs to reach the app search FlowBox above.
pub(super) fn install_file_results_nav(container: &gtk4::Box) {
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
