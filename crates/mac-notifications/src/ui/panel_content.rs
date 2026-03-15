use super::notification_row;
use crate::state::NotificationState;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Rebuilds the panel's notification list, grouped by app.
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

        let icon = resolve_group_icon(&group.app_icon, &group.app_name, &app_dirs);
        icon.set_pixel_size(super::constants::GROUP_ICON_SIZE);
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

        // Notification rows
        for notif in &group.notifications {
            let click_cb = Rc::clone(&on_notification_click);
            let state_click = Rc::clone(state);
            let rebuild_click = Rc::clone(&on_rebuild);
            let state_dismiss = Rc::clone(state);
            let rebuild_dismiss = Rc::clone(&on_rebuild);

            let row = notification_row::build_row(
                notif,
                &app_dirs,
                move |id| {
                    click_cb(id);
                    // Remove from history after focusing app (like macOS)
                    state_click.borrow_mut().remove(id);
                    rebuild_click();
                },
                move |id| {
                    state_dismiss.borrow_mut().remove(id);
                    rebuild_dismiss();
                },
            );
            container.append(&row);
        }

        // Separator between groups
        let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        sep.set_margin_top(4);
        sep.set_margin_bottom(4);
        container.append(&sep);
    }
}

/// Resolves group icon using GTK4 icon theme (avoids glycin crashes).
fn resolve_group_icon(
    app_icon: &str,
    app_name: &str,
    app_dirs: &[std::path::PathBuf],
) -> gtk4::Image {
    use dock_common::desktop::icons;
    let size = super::constants::GROUP_ICON_SIZE;

    if !app_icon.is_empty() && !app_icon.contains('/') {
        let img = gtk4::Image::from_icon_name(app_icon);
        img.set_pixel_size(size);
        return img;
    }
    if let Some(icon_name) = icons::get_icon(app_name, app_dirs)
        && !icon_name.contains('/')
    {
        let img = gtk4::Image::from_icon_name(&icon_name);
        img.set_pixel_size(size);
        return img;
    }
    let img = gtk4::Image::from_icon_name("dialog-information");
    img.set_pixel_size(size);
    img
}
