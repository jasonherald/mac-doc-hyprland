use crate::state::DrawerState;
use dock_common::desktop::categories;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Builds the category button bar.
/// Currently unused in the unified well design but kept for future use.
#[allow(dead_code)]
pub fn build_category_bar(
    state: &Rc<RefCell<DrawerState>>,
    on_category_selected: Rc<dyn Fn(Option<Vec<String>>)>,
) -> gtk4::Box {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);

    // "All" button
    let all_btn = gtk4::Button::with_label("All");
    all_btn.set_widget_name("category-button");
    let cb = Rc::clone(&on_category_selected);
    all_btn.connect_clicked(move |_| {
        cb(None);
    });
    hbox.append(&all_btn);

    // Category buttons
    let cats = categories::default_categories();
    for cat in &cats {
        let cat_name = cat.name.clone();
        let has_entries = {
            let s = state.borrow();
            s.category_lists
                .get(&cat_name)
                .is_some_and(|list| list.iter().any(|id| {
                    s.id2entry.get(id).is_some_and(|e| !e.no_display)
                }))
        };

        if !has_entries {
            continue;
        }

        let button = gtk4::Button::with_label(&cat.display_name);
        button.set_widget_name("category-button");

        let cat_name_click = cat_name.clone();
        let state_ref = Rc::clone(state);
        let cb = Rc::clone(&on_category_selected);
        button.connect_clicked(move |_| {
            let list = state_ref
                .borrow()
                .category_lists
                .get(&cat_name_click)
                .cloned()
                .unwrap_or_default();
            cb(Some(list));
        });

        hbox.append(&button);
    }

    hbox
}
