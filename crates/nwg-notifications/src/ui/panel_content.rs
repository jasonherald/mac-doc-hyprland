use super::notification_row;
use crate::state::NotificationState;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Maximum notifications to show per app group before collapsing.
const MAX_VISIBLE_PER_GROUP: usize = 3;

/// Rebuilds the panel's notification list, grouped by app.
///
/// Groups with more than MAX_VISIBLE_PER_GROUP notifications start collapsed,
/// showing only the latest few. Click the group header to expand/collapse.
pub fn build_grouped_list(
    container: &gtk4::Box,
    state: &Rc<RefCell<NotificationState>>,
    on_notification_click: Rc<dyn Fn(u32)>,
    on_rebuild: Rc<dyn Fn()>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let groups = state.borrow().grouped_by_app();

    if groups.is_empty() {
        let empty = gtk4::Label::new(Some("No notifications"));
        empty.add_css_class("panel-empty");
        empty.set_margin_top(40);
        container.append(&empty);
        return;
    }

    let app_dirs = state.borrow().app_dirs.clone();

    for group in &groups {
        build_group(
            container,
            group,
            &app_dirs,
            state,
            &on_notification_click,
            &on_rebuild,
        );
    }
}

/// Builds a single app group: header + notification rows + collapse toggle.
#[allow(clippy::too_many_arguments)]
fn build_group(
    container: &gtk4::Box,
    group: &crate::state::AppGroup,
    app_dirs: &[std::path::PathBuf],
    state: &Rc<RefCell<NotificationState>>,
    on_notification_click: &Rc<dyn Fn(u32)>,
    on_rebuild: &Rc<dyn Fn()>,
) {
    let total = group.notifications.len();
    let should_collapse = total > MAX_VISIBLE_PER_GROUP;

    // --- Group header (clickable to toggle collapse) ---
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    header.add_css_class("group-header");

    let icon = super::icons::resolve_theme_icon(
        &group.app_icon,
        &group.app_name,
        app_dirs,
        super::constants::GROUP_ICON_SIZE,
    );
    header.append(&icon);

    let name_label = gtk4::Label::new(Some(&group.app_name));
    name_label.add_css_class("group-name");
    name_label.set_hexpand(true);
    name_label.set_halign(gtk4::Align::Start);
    header.append(&name_label);

    // Collapse indicator: shows count and arrow
    let collapse_text = if should_collapse {
        format!("{} \u{25BC}", total) // ▼ collapsed
    } else {
        format!("{}", total)
    };
    let count_label = gtk4::Label::new(Some(&collapse_text));
    count_label.add_css_class("group-count");
    header.append(&count_label);

    // Dismiss all for this app
    let dismiss_group = gtk4::Button::from_icon_name("edit-clear-symbolic");
    dismiss_group.add_css_class("group-dismiss");
    dismiss_group.set_tooltip_text(Some("Dismiss all"));
    let app_name = group.app_name.clone();
    let state_dismiss = Rc::clone(state);
    let rebuild = Rc::clone(on_rebuild);
    dismiss_group.connect_clicked(move |_| {
        state_dismiss.borrow_mut().dismiss_app(&app_name);
        rebuild();
    });
    header.append(&dismiss_group);

    container.append(&header);

    // --- Visible rows (always shown) ---
    let visible_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    // --- Overflow rows (hidden when collapsed) ---
    let overflow_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    overflow_box.set_visible(false);

    for (i, notif) in group.notifications.iter().enumerate() {
        let click_cb = Rc::clone(on_notification_click);
        let state_click = Rc::clone(state);
        let rebuild_click = Rc::clone(on_rebuild);
        let state_dismiss_row = Rc::clone(state);
        let rebuild_dismiss = Rc::clone(on_rebuild);

        let row = notification_row::build_row(
            notif,
            app_dirs,
            move |id| {
                click_cb(id);
                state_click.borrow_mut().remove(id);
                rebuild_click();
            },
            move |id| {
                state_dismiss_row.borrow_mut().remove(id);
                rebuild_dismiss();
            },
        );

        if !should_collapse || i < MAX_VISIBLE_PER_GROUP {
            visible_box.append(&row);
        } else {
            overflow_box.append(&row);
        }
    }

    container.append(&visible_box);
    container.append(&overflow_box);

    // --- Click header to toggle collapse ---
    if should_collapse {
        let overflow_ref = overflow_box;
        let count_ref = count_label;
        let total_count = total;
        let expanded = Rc::new(RefCell::new(false));

        let click = gtk4::GestureClick::new();
        click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            let mut is_expanded = expanded.borrow_mut();
            *is_expanded = !*is_expanded;
            overflow_ref.set_visible(*is_expanded);
            let arrow = if *is_expanded { "\u{25B2}" } else { "\u{25BC}" };
            count_ref.set_text(&format!("{} {}", total_count, arrow));
        });
        header.add_controller(click);
    }

    // Separator between groups
    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    sep.set_margin_top(4);
    sep.set_margin_bottom(4);
    container.append(&sep);
}
