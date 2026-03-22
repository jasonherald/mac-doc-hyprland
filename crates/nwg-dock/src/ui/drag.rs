//! Manual drag-to-reorder for dock pinned items.
//!
//! Uses `GestureDrag` instead of GTK4's `DragSource`/`DropTarget` to avoid
//! SIGSEGV crashes in GTK4's DnD signal emission on Wayland layer-shell
//! surfaces (GNOME/gtk#3566, #3090). GestureDrag tracks press-move-release
//! via raw pointer events without creating a GdkDrag object.
//!
//! The actual dock items are reordered live as you drag — no placeholder
//! or preview copies needed. Dragging outside the dock shows a removal
//! indicator and changes the cursor.

use crate::state::DockState;
use gtk4::prelude::*;
use nwg_dock_common::pinning;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Pixels outside the dock box before we consider the cursor "outside"
/// for the unpin-by-drag-off behavior.
const OUTSIDE_MARGIN: f64 = 30.0;

/// Transient state for an active drag operation.
struct DragSession {
    source_index: usize,
    /// Current position of the dragged item (changes as items are reordered live).
    current_index: usize,
    /// Cursor position in dock_box coordinates at drag start.
    dock_start_x: f64,
    dock_start_y: f64,
    dock_box: gtk4::Box,
    /// The source item widget (for CSS class toggling).
    source_item: gtk4::Widget,
    vertical: bool,
    /// Scaled icon size (to match removal icon to app icon size).
    icon_size: i32,
}

/// Attaches manual drag-to-reorder behavior to a pinned button.
///
/// Uses `GestureDrag` (button 1) which participates in GTK4's gesture
/// competition: click without movement → app launches normally;
/// drag past threshold → reorder begins, click is suppressed.
pub fn setup_drag_gesture(
    button: &gtk4::Button,
    index: usize,
    vertical: bool,
    state: &Rc<RefCell<DockState>>,
    pinned_file: &Path,
    rebuild: &Rc<dyn Fn()>,
) {
    let session: Rc<RefCell<Option<DragSession>>> = Rc::new(RefCell::new(None));

    let gesture = gtk4::GestureDrag::new();
    gesture.set_button(1);

    // --- drag-begin ---
    let state_begin = Rc::clone(state);
    let session_begin = Rc::clone(&session);
    let vert = vertical;
    gesture.connect_drag_begin(move |gesture, start_x, start_y| {
        let Some(widget) = gesture.widget() else {
            return;
        };
        let Some(item_box) = widget.parent() else {
            return;
        };
        let Some(dock_box_widget) = item_box.parent() else {
            return;
        };
        let Ok(dock_box) = dock_box_widget.downcast::<gtk4::Box>() else {
            return;
        };

        let (dock_x, dock_y) = match widget.translate_coordinates(&dock_box, start_x, start_y) {
            Some(coords) => coords,
            None => return,
        };

        state_begin.borrow_mut().drag_source_index = Some(index);
        let icon_size = state_begin.borrow().img_size_scaled;

        // Claim the gesture sequence so GestureClick doesn't fire on release
        gesture.set_state(gtk4::EventSequenceState::Claimed);

        // Set grabbing cursor on the dock window
        if let Some(root) = dock_box.root() {
            let cursor = gtk4::gdk::Cursor::from_name("grabbing", None);
            root.upcast_ref::<gtk4::Widget>()
                .set_cursor(cursor.as_ref());
        }

        *session_begin.borrow_mut() = Some(DragSession {
            source_index: index,
            current_index: index,
            dock_start_x: dock_x,
            dock_start_y: dock_y,
            dock_box,
            source_item: item_box,
            vertical: vert,
            icon_size,
        });
    });

    // --- drag-update: reorder items live, track inside/outside ---
    let state_update = Rc::clone(state);
    let session_update = Rc::clone(&session);
    gesture.connect_drag_update(move |_gesture, offset_x, offset_y| {
        let mut sess = session_update.borrow_mut();
        let Some(ref mut s) = *sess else { return };

        let current_x = s.dock_start_x + offset_x;
        let current_y = s.dock_start_y + offset_y;
        let coord = if s.vertical { current_y } else { current_x };

        // Calculate where the item should be and reorder live
        let target_idx = calculate_drop_index(&s.dock_box, coord, s.vertical, &s.source_item);
        if target_idx != s.current_index {
            move_child_to_index(&s.dock_box, &s.source_item, target_idx);
            s.current_index = target_idx;
        }

        // Track inside/outside dock
        let outside = is_cursor_outside_dock(&s.dock_box, current_x, current_y, s.vertical);
        state_update.borrow_mut().drag_outside_dock = outside;

        // Visual feedback: swap icon to X when outside, update cursor
        update_removal_indicator(&s.source_item, outside, s.icon_size);
        set_dock_cursor(
            &s.dock_box,
            if outside { "not-allowed" } else { "grabbing" },
        );
    });

    // --- drag-end: save new order or unpin ---
    let state_end = Rc::clone(state);
    let session_end = Rc::clone(&session);
    let pinned_path = pinned_file.to_path_buf();
    let rebuild = Rc::clone(rebuild);
    gesture.connect_drag_end(move |_gesture, _offset_x, _offset_y| {
        let sess = session_end.borrow_mut().take();
        let Some(s) = sess else { return };

        let outside = state_end.borrow().drag_outside_dock;

        // Clear drag state
        state_end.borrow_mut().drag_source_index = None;
        state_end.borrow_mut().drag_outside_dock = false;

        // Restore cursor and visuals
        if let Some(root) = s.dock_box.root() {
            root.upcast_ref::<gtk4::Widget>().set_cursor(None);
        }
        update_removal_indicator(&s.source_item, false, 0);

        finalize_drag(&state_end, &s, outside, &pinned_path, &rebuild);
    });

    button.add_controller(gesture);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sets the cursor on the dock's toplevel window.
fn set_dock_cursor(dock_box: &gtk4::Box, cursor_name: &str) {
    if let Some(root) = dock_box.root() {
        let cursor = gtk4::gdk::Cursor::from_name(cursor_name, None);
        root.upcast_ref::<gtk4::Widget>()
            .set_cursor(cursor.as_ref());
    }
}

/// Finalizes a drag operation: either unpins, reorders, or cancels.
fn finalize_drag(
    state: &Rc<RefCell<DockState>>,
    session: &DragSession,
    outside: bool,
    pinned_path: &Path,
    rebuild: &Rc<dyn Fn()>,
) {
    if outside {
        unpin_by_drag(state, session.source_index, pinned_path, rebuild);
    } else if session.current_index != session.source_index {
        reorder_pinned(state, session, pinned_path, rebuild);
    }
}

/// Removes a pinned item by index and saves.
fn unpin_by_drag(
    state: &Rc<RefCell<DockState>>,
    source_index: usize,
    pinned_path: &Path,
    rebuild: &Rc<dyn Fn()>,
) {
    let mut st = state.borrow_mut();
    if source_index < st.pinned.len() {
        let removed = st.pinned.remove(source_index);
        log::info!("Unpinned by drag-off: {}", removed);
        if let Err(e) = pinning::save_pinned(&st.pinned, pinned_path) {
            log::error!("Failed to save pins: {}", e);
        }
        drop(st);
        let rebuild = Rc::clone(rebuild);
        gtk4::glib::idle_add_local_once(move || rebuild());
    }
}

/// Reorders the pinned list to match the visual order after drag.
fn reorder_pinned(
    state: &Rc<RefCell<DockState>>,
    session: &DragSession,
    pinned_path: &Path,
    rebuild: &Rc<dyn Fn()>,
) {
    let mut st = state.borrow_mut();
    let pinned_len = st.pinned.len();
    if session.source_index < pinned_len && session.current_index <= pinned_len {
        let item = st.pinned.remove(session.source_index);
        let adjusted = if session.current_index > session.source_index {
            session.current_index - 1
        } else {
            session.current_index
        };
        st.pinned.insert(adjusted, item);
        if let Err(e) = pinning::save_pinned(&st.pinned, pinned_path) {
            log::error!("Failed to save reordered pins: {}", e);
        }
        drop(st);
        let rebuild = Rc::clone(rebuild);
        gtk4::glib::idle_add_local_once(move || rebuild());
    }
}

/// Shows/hides the removal indicator by swapping the button's image content.
/// Saves the original image as widget data so it can be restored.
fn update_removal_indicator(item: &gtk4::Widget, outside: bool, icon_size: i32) {
    // The item_box contains: [Button [Image], Indicator]
    // The button's child is the Image we need to swap
    let Some(button_widget) = item.first_child() else {
        return;
    };
    let Ok(button) = button_widget.clone().downcast::<gtk4::Button>() else {
        return;
    };

    if outside && !item.has_css_class("drag-will-remove") {
        item.add_css_class("drag-will-remove");

        // Save the entire button child and replace with a clean trash icon
        if let Some(original) = button.child() {
            // SAFETY: We own `item` for the duration of the drag. The stored widget
            // is retrieved in the else branch below with matching key and type.
            unsafe {
                item.set_data("drag-original-child", original);
            }
        }
        // Match the original icon size so the dock doesn't resize
        let remove_icon = gtk4::Image::from_icon_name("window-close-symbolic");
        remove_icon.set_pixel_size(icon_size);
        remove_icon.set_halign(gtk4::Align::Center);
        remove_icon.set_valign(gtk4::Align::Center);
        remove_icon.add_css_class("drag-remove-icon");
        button.set_child(Some(&remove_icon));
        // Hide the indicator dot below the button
        if let Some(indicator) = button_widget.next_sibling() {
            indicator.set_visible(false);
        }
    } else if !outside && item.has_css_class("drag-will-remove") {
        item.remove_css_class("drag-will-remove");

        // Restore original button content
        // SAFETY: Data was set in the if-outside branch above with matching key and type.
        let original: Option<gtk4::Widget> = unsafe { item.steal_data("drag-original-child") };
        if let Some(orig) = original {
            button.set_child(Some(&orig));
        }
        // Restore indicator dot
        if let Some(indicator) = button_widget.next_sibling() {
            indicator.set_visible(true);
        }
    }
}

/// Returns true if the cursor position is outside the dock box bounds.
fn is_cursor_outside_dock(dock_box: &gtk4::Box, x: f64, y: f64, vertical: bool) -> bool {
    let w = dock_box.width() as f64;
    let h = dock_box.height() as f64;
    if vertical {
        x < -OUTSIDE_MARGIN || x > w + OUTSIDE_MARGIN
    } else {
        y < -OUTSIDE_MARGIN || y > h + OUTSIDE_MARGIN
    }
}

/// Calculates which index the dragged item should be at based on cursor position.
/// Only counts pinned-item children, skips the dragged item itself.
fn calculate_drop_index(
    dock_box: &gtk4::Box,
    coord: f64,
    vertical: bool,
    dragged: &gtk4::Widget,
) -> usize {
    let mut positions = Vec::new();
    let mut child = dock_box.first_child();

    while let Some(widget) = child {
        // Skip the dragged item and non-pinned items
        if widget != *dragged && widget.has_css_class("pinned-item") {
            let alloc = widget.allocation();
            let center = if vertical {
                alloc.y() as f64 + alloc.height() as f64 / 2.0
            } else {
                alloc.x() as f64 + alloc.width() as f64 / 2.0
            };
            positions.push(center);
        }
        child = widget.next_sibling();
    }

    for (i, &center) in positions.iter().enumerate() {
        if coord < center {
            return i;
        }
    }
    positions.len()
}

/// Moves a child widget to a specific index position in the dock box.
fn move_child_to_index(dock_box: &gtk4::Box, child_widget: &gtk4::Widget, target_index: usize) {
    // Find the widget currently at target_index (among pinned items, excluding the moved one)
    let mut pinned_children = Vec::new();
    let mut child = dock_box.first_child();
    while let Some(widget) = child {
        if widget.has_css_class("pinned-item") && widget != *child_widget {
            pinned_children.push(widget.clone());
        }
        child = widget.next_sibling();
    }

    if target_index == 0 {
        // Move before the first pinned item (or to the start if none)
        if let Some(first) = pinned_children.first() {
            dock_box.reorder_child_after(child_widget, first.prev_sibling().as_ref());
        }
    } else if target_index <= pinned_children.len() {
        // Move after the item at target_index - 1
        let after = &pinned_children[target_index - 1];
        dock_box.reorder_child_after(child_widget, Some(after));
    } else {
        // Move to end of pinned items
        if let Some(last) = pinned_children.last() {
            dock_box.reorder_child_after(child_widget, Some(last));
        }
    }
}
