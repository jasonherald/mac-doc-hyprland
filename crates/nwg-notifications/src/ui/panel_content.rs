use super::notification_row;
use crate::state::NotificationState;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Maximum notifications to show per app group before collapsing.
const MAX_VISIBLE_PER_GROUP: usize = 3;

/// Rebuilds the panel's notification list, grouped by app.
///
/// Groups with more than MAX_VISIBLE_PER_GROUP notifications are collapsed
/// to show only the latest few, with a "N more" button to expand.
pub fn build_grouped_list(
    container: &gtk4::Box,
    state: &Rc<RefCell<NotificationState>>,
    on_notification_click: Rc<dyn Fn(u32)>,
    on_rebuild: Rc<dyn Fn()>,
) {
    // Clear existing content
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
        // Group header: icon + app name + count + dismiss-group button
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        header.add_css_class("group-header");

        let icon = super::icons::resolve_theme_icon(
            &group.app_icon,
            &group.app_name,
            &app_dirs,
            super::constants::GROUP_ICON_SIZE,
        );
        header.append(&icon);

        let name_label = gtk4::Label::new(Some(&group.app_name));
        name_label.add_css_class("group-name");
        name_label.set_hexpand(true);
        name_label.set_halign(gtk4::Align::Start);
        header.append(&name_label);

        let count = gtk4::Label::new(Some(&format!("{}", group.notifications.len())));
        count.add_css_class("group-count");
        header.append(&count);

        // Dismiss all for this app
        let dismiss_group = gtk4::Button::from_icon_name("edit-clear-symbolic");
        dismiss_group.add_css_class("group-dismiss");
        dismiss_group.set_tooltip_text(Some("Dismiss all"));
        let app_name = group.app_name.clone();
        let state_dismiss = Rc::clone(state);
        let rebuild = Rc::clone(&on_rebuild);
        dismiss_group.connect_clicked(move |_| {
            state_dismiss.borrow_mut().dismiss_app(&app_name);
            rebuild();
        });
        header.append(&dismiss_group);

        container.append(&header);

        // Notification rows — collapse large groups
        let total = group.notifications.len();
        let collapsed = total > MAX_VISIBLE_PER_GROUP;
        let visible_count = if collapsed {
            MAX_VISIBLE_PER_GROUP
        } else {
            total
        };

        // Container for the overflow rows (hidden initially when collapsed)
        let overflow_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        overflow_box.set_visible(!collapsed || visible_count == total);

        for (i, notif) in group.notifications.iter().enumerate() {
            let click_cb = Rc::clone(&on_notification_click);
            let state_click = Rc::clone(state);
            let rebuild_click = Rc::clone(&on_rebuild);
            let state_dismiss_row = Rc::clone(state);
            let rebuild_dismiss = Rc::clone(&on_rebuild);

            let row = notification_row::build_row(
                notif,
                &app_dirs,
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

            if i < visible_count {
                container.append(&row);
            } else {
                overflow_box.append(&row);
            }
        }

        // "N more" expand button
        if collapsed {
            let remaining = total - visible_count;
            let expand_btn = gtk4::Button::with_label(&format!("{} more", remaining));
            expand_btn.add_css_class("group-expand");
            expand_btn.set_has_frame(false);

            let overflow_ref = overflow_box.clone();
            expand_btn.connect_clicked(move |btn| {
                overflow_ref.set_visible(true);
                btn.set_visible(false);
            });

            container.append(&expand_btn);
            container.append(&overflow_box);
        }

        // Separator between groups
        let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        sep.set_margin_top(4);
        sep.set_margin_bottom(4);
        container.append(&sep);
    }
}
