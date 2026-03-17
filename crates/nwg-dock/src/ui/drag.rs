use crate::state::DockState;
use gtk4::prelude::*;
use nwg_dock_common::desktop::icons;
use nwg_dock_common::pinning;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Name for the placeholder widget inserted at the drop zone.
const PLACEHOLDER_NAME: &str = "drag-placeholder";

/// Attaches drag source behavior to a pinned button.
///
/// On drag begin: hides the original icon (looks like you picked it up).
/// On drag end outside dock: unpins the item (drag-to-remove, like macOS).
pub fn setup_drag_source(
    button: &gtk4::Button,
    index: usize,
    state: &Rc<RefCell<DockState>>,
    pinned_file: &Path,
    rebuild: &Rc<dyn Fn()>,
) {
    let drag_source = gtk4::DragSource::new();
    drag_source.set_actions(gtk4::gdk::DragAction::MOVE);

    // Prepare: provide drag data and snapshot the icon immediately
    let state_prepare = Rc::clone(state);
    drag_source.connect_prepare(move |source, _x, _y| {
        // Snapshot the icon NOW (before any visual changes)
        if let Some(widget) = source.widget() {
            let paintable = gtk4::WidgetPaintable::new(Some(&widget));
            source.set_icon(Some(&paintable), 0, 0);
        }
        state_prepare.borrow_mut().drag_source_index = Some(index);
        let value = (index as u32).to_value();
        Some(gtk4::gdk::ContentProvider::for_value(&value))
    });

    // Drag begin: fade the original and mark it as being dragged
    drag_source.connect_drag_begin(move |source, _drag| {
        if let Some(widget) = source.widget()
            && let Some(parent) = widget.parent()
        {
            parent.set_opacity(0.2);
            parent.add_css_class("dragging-source");
        }
    });

    // Drag end: if dropped outside the dock (not accepted), unpin the item
    let state_end = Rc::clone(state);
    let pinned_path = pinned_file.to_path_buf();
    let rebuild = Rc::clone(rebuild);
    drag_source.connect_drag_end(move |source, _drag, _delete_data| {
        let drag_idx = state_end.borrow().drag_source_index;
        state_end.borrow_mut().drag_source_index = None;

        // Restore the item's appearance
        if let Some(widget) = source.widget()
            && let Some(parent) = widget.parent()
        {
            parent.set_opacity(1.0);
            parent.remove_css_class("dragging-source");
            parent.remove_css_class("drag-will-remove");
        }

        // Only unpin if cursor was OUTSIDE the dock area at time of release.
        // We use drag_outside_dock (set by cursor poller) instead of delete_data,
        // because GTK4 may not properly re-establish drop targets after leave+re-enter.
        let outside = state_end.borrow().drag_outside_dock;
        state_end.borrow_mut().drag_outside_dock = false;

        if outside && let Some(idx) = drag_idx {
            let mut s = state_end.borrow_mut();
            if idx < s.pinned.len() {
                let removed = s.pinned.remove(idx);
                log::info!("Unpinned by drag-off: {}", removed);
                if let Err(e) = pinning::save_pinned(&s.pinned, &pinned_path) {
                    log::error!("Failed to save pins: {}", e);
                }
                drop(s);
                // Defer rebuild to next idle cycle to avoid glycin reentrancy
                let rebuild = Rc::clone(&rebuild);
                gtk4::glib::idle_add_local_once(move || rebuild());
            }
        }
    });

    button.add_controller(drag_source);
}

/// Attaches a drop target to the main dock box.
///
/// Shows a semi-transparent copy of the dragged icon at the calculated
/// drop position, so the user can see exactly where it will land.
pub fn setup_dock_drop_target(
    dock_box: &gtk4::Box,
    icon_size: i32,
    state: &Rc<RefCell<DockState>>,
    pinned_file: &Path,
    rebuild: &Rc<dyn Fn()>,
) {
    let drop_target = gtk4::DropTarget::new(u32::static_type(), gtk4::gdk::DragAction::MOVE);

    let current_placeholder_idx: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));

    // Create the preview icon ONCE (cached), not on every motion event.
    // This avoids calling glycin image loader on every mouse move.
    let cached_preview: Rc<RefCell<Option<gtk4::Box>>> = Rc::new(RefCell::new(None));

    let dock_box_motion = dock_box.clone();
    let placeholder_idx_motion = Rc::clone(&current_placeholder_idx);
    let state_motion = Rc::clone(state);
    let cached_preview_motion = Rc::clone(&cached_preview);
    let size = icon_size;

    drop_target.connect_motion(move |_target, x, _y| {
        // Clear "will remove" indicator when cursor returns to dock
        if let Some(idx) = state_motion.borrow().drag_source_index
            && let Some(child) = find_dock_child_at(&dock_box_motion, idx)
        {
            child.remove_css_class("drag-will-remove");
            child.set_opacity(0.2);
        }

        let new_idx = calculate_drop_index(&dock_box_motion, x);
        let mut current = placeholder_idx_motion.borrow_mut();

        if *current != Some(new_idx) {
            remove_placeholder(&dock_box_motion);

            // Create preview once, reuse on subsequent moves
            let preview = {
                let mut cache = cached_preview_motion.borrow_mut();
                if cache.is_none() {
                    let s = state_motion.borrow();
                    *cache = Some(create_icon_preview(&s, size));
                }
                // Safe: we just ensured cache is Some above
                cache.as_ref().unwrap().clone()
            };

            // Unparent if previously parented elsewhere
            if let Some(parent) = preview.parent()
                && let Ok(parent_box) = parent.downcast::<gtk4::Box>()
            {
                parent_box.remove(&preview);
            }

            insert_placeholder(&dock_box_motion, new_idx, preview);
            *current = Some(new_idx);
        }

        gtk4::gdk::DragAction::MOVE
    });

    // Remove placeholder when drag leaves dock — show "will remove" on source
    let dock_box_leave = dock_box.clone();
    let placeholder_leave = Rc::clone(&current_placeholder_idx);
    let state_leave = Rc::clone(state);
    let cached_preview_leave = Rc::clone(&cached_preview);
    drop_target.connect_leave(move |_target| {
        remove_placeholder(&dock_box_leave);
        *placeholder_leave.borrow_mut() = None;
        *cached_preview_leave.borrow_mut() = None; // drop cached preview

        // Mark the source item as "will be removed"
        if let Some(idx) = state_leave.borrow().drag_source_index
            && let Some(child) = find_dock_child_at(&dock_box_leave, idx)
        {
            child.add_css_class("drag-will-remove");
            child.set_opacity(0.3);
        }
    });

    // On drop: reorder pinned list
    let dock_box_drop = dock_box.clone();
    let placeholder_drop = Rc::clone(&current_placeholder_idx);
    let state = Rc::clone(state);
    let pinned_path = pinned_file.to_path_buf();
    let rebuild = Rc::clone(rebuild);

    drop_target.connect_drop(move |_target, value, _x, _y| {
        remove_placeholder(&dock_box_drop);
        let target_idx = placeholder_drop.borrow_mut().take();

        let from_index = match value.get::<u32>() {
            Ok(i) => i as usize,
            Err(_) => return false,
        };

        let target_index = match target_idx {
            Some(i) => i,
            None => return false,
        };

        if from_index == target_index {
            return false;
        }

        let mut s = state.borrow_mut();
        let pinned_len = s.pinned.len();
        if from_index >= pinned_len || target_index > pinned_len {
            return false;
        }

        let adjusted_target = if target_index > from_index {
            target_index - 1
        } else {
            target_index
        };

        let item = s.pinned.remove(from_index);
        s.pinned.insert(adjusted_target, item);

        if let Err(e) = pinning::save_pinned(&s.pinned, &pinned_path) {
            log::error!("Failed to save reordered pins: {}", e);
        }

        drop(s);
        // Defer rebuild to next idle cycle to avoid glycin reentrancy
        let rebuild = Rc::clone(&rebuild);
        gtk4::glib::idle_add_local_once(move || rebuild());
        true
    });

    dock_box.add_controller(drop_target);
}

/// Creates a semi-transparent copy of the dragged icon for the drop zone.
fn create_icon_preview(state: &DockState, icon_size: i32) -> gtk4::Box {
    let preview = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    preview.set_widget_name(PLACEHOLDER_NAME);
    preview.set_opacity(0.5);

    // Look up the dragged app's icon
    let app_id = state
        .drag_source_index
        .and_then(|idx| state.pinned.get(idx))
        .cloned();

    if let Some(app_id) = app_id
        && let Some(image) = icons::create_image(&app_id, icon_size, &state.app_dirs)
    {
        image.set_pixel_size(icon_size);
        preview.append(&image);
        return preview;
    }

    // Fallback: generic icon
    let image = gtk4::Image::from_icon_name("application-x-executable");
    image.set_pixel_size(icon_size);
    preview.append(&image);
    preview
}

/// Calculates which index a drop at position `x` should insert at.
fn calculate_drop_index(dock_box: &gtk4::Box, x: f64) -> usize {
    let mut child_positions = Vec::new();
    let mut child = dock_box.first_child();

    while let Some(widget) = child {
        if widget.widget_name().as_str() != PLACEHOLDER_NAME {
            let alloc = widget.allocation();
            let center = alloc.x() as f64 + alloc.width() as f64 / 2.0;
            child_positions.push(center);
        }
        child = widget.next_sibling();
    }

    for (i, &center) in child_positions.iter().enumerate() {
        if x < center {
            return i;
        }
    }

    child_positions.len()
}

/// Inserts a placeholder widget at the given index.
fn insert_placeholder(dock_box: &gtk4::Box, index: usize, placeholder: gtk4::Box) {
    let mut child = dock_box.first_child();
    let mut real_idx = 0;
    while let Some(widget) = &child {
        if widget.widget_name().as_str() == PLACEHOLDER_NAME {
            child = widget.next_sibling();
            continue;
        }
        if real_idx == index {
            dock_box.insert_child_after(&placeholder, widget.prev_sibling().as_ref());
            return;
        }
        real_idx += 1;
        child = widget.next_sibling();
    }
    dock_box.append(&placeholder);
}

/// Removes all placeholder widgets from the dock box.
fn remove_placeholder(dock_box: &gtk4::Box) {
    let mut child = dock_box.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if widget.widget_name().as_str() == PLACEHOLDER_NAME {
            dock_box.remove(&widget);
        }
        child = next;
    }
}

/// Finds the Nth real (non-placeholder) child in the dock box.
fn find_dock_child_at(dock_box: &gtk4::Box, index: usize) -> Option<gtk4::Widget> {
    let mut child = dock_box.first_child();
    let mut real_idx = 0;
    while let Some(widget) = child {
        if widget.widget_name().as_str() != PLACEHOLDER_NAME {
            if real_idx == index {
                return Some(widget);
            }
            real_idx += 1;
        }
        child = widget.next_sibling();
    }
    None
}
