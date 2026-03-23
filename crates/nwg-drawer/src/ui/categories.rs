use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui;
use gtk4::prelude::*;
use nwg_dock_common::desktop::categories::default_categories;
use std::cell::RefCell;
use std::rc::Rc;

/// Builds the category filter button bar.
#[allow(clippy::too_many_arguments)]
pub fn build_category_bar(
    config: &Rc<DrawerConfig>,
    state: &Rc<RefCell<DrawerState>>,
    well: &gtk4::Box,
    pinned_box: &gtk4::Box,
    pinned_file: &Rc<std::path::PathBuf>,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
    search_entry: &gtk4::SearchEntry,
) -> gtk4::Box {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    hbox.add_css_class("category-bar");
    hbox.set_halign(gtk4::Align::Center);
    hbox.set_margin_top(super::constants::CATEGORY_BAR_TOP_MARGIN);
    hbox.set_margin_bottom(super::constants::CATEGORY_BAR_BOTTOM_MARGIN);

    let buttons: Rc<RefCell<Vec<gtk4::Button>>> = Rc::new(RefCell::new(Vec::new()));

    // "All" button — restores full view
    let all_btn = gtk4::Button::with_label("All");
    all_btn.add_css_class("category-button");
    all_btn.add_css_class("category-selected");
    all_btn.set_widget_name("category-button");

    {
        let well = well.clone();
        let pinned_box = pinned_box.clone();
        let config = Rc::clone(config);
        let state = Rc::clone(state);
        let pinned_file = Rc::clone(pinned_file);
        let on_launch = Rc::clone(on_launch);
        let buttons = Rc::clone(&buttons);
        let status_label = status_label.clone();
        let search_entry = search_entry.clone();
        all_btn.connect_clicked(move |btn| {
            select_button(btn, &buttons);
            search_entry.set_text("");
            state.borrow_mut().active_search.clear();
            state.borrow_mut().active_category.clear();
            pinned_box.set_visible(true);
            ui::well_builder::build_normal_well(
                &well,
                &pinned_box,
                &config,
                &state,
                &pinned_file,
                &on_launch,
                &status_label,
            );
        });
    }
    hbox.append(&all_btn);
    buttons.borrow_mut().push(all_btn);

    // Category buttons
    let categories = default_categories();
    let cat_lists = state.borrow().apps.category_lists.clone();

    for cat in &categories {
        if cat.name == "Other" {
            continue;
        }
        let ids = match cat_lists.get(&cat.name) {
            Some(ids) if !ids.is_empty() => ids.clone(),
            _ => continue,
        };

        let btn = create_category_button(
            &cat.display_name,
            &cat.icon,
            ids,
            config,
            state,
            well,
            pinned_box,
            pinned_file,
            on_launch,
            &buttons,
            status_label,
            search_entry,
        );
        hbox.append(&btn);
        buttons.borrow_mut().push(btn);
    }

    hbox
}

/// Creates a single category filter button.
#[allow(clippy::too_many_arguments)]
fn create_category_button(
    display_name: &str,
    icon_name: &str,
    ids: Vec<String>,
    config: &Rc<DrawerConfig>,
    state: &Rc<RefCell<DrawerState>>,
    well: &gtk4::Box,
    pinned_box: &gtk4::Box,
    pinned_file: &Rc<std::path::PathBuf>,
    on_launch: &Rc<dyn Fn()>,
    buttons: &Rc<RefCell<Vec<gtk4::Button>>>,
    status_label: &gtk4::Label,
    search_entry: &gtk4::SearchEntry,
) -> gtk4::Button {
    let btn = gtk4::Button::new();
    let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    let icon = gtk4::Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    btn_box.append(&icon);
    btn_box.append(&gtk4::Label::new(Some(display_name)));
    btn.set_child(Some(&btn_box));
    btn.add_css_class("category-button");
    btn.set_widget_name("category-button");

    let well = well.clone();
    let pinned_box = pinned_box.clone();
    let config = Rc::clone(config);
    let state = Rc::clone(state);
    let pinned_file = Rc::clone(pinned_file);
    let on_launch = Rc::clone(on_launch);
    let buttons = Rc::clone(buttons);
    let status_label = status_label.clone();
    let search_entry = search_entry.clone();
    btn.connect_clicked(move |btn| {
        select_button(btn, &buttons);
        search_entry.set_text("");
        state.borrow_mut().active_search.clear();
        state.borrow_mut().active_category = ids.clone();
        apply_category_filter(
            &well,
            &pinned_box,
            &config,
            &state,
            &ids,
            &pinned_file,
            &on_launch,
            &status_label,
        );
    });

    btn
}

/// Builds the well content filtered to a specific category.
/// Public so the rebuild callback can restore the active filter after unpin.
#[allow(clippy::too_many_arguments)]
pub fn apply_category_filter(
    well: &gtk4::Box,
    pinned_box: &gtk4::Box,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    category_ids: &[String],
    pinned_file: &std::path::Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) {
    while let Some(child) = well.first_child() {
        well.remove(&child);
    }

    let on_rebuild = ui::well_builder::build_rebuild_callback(
        well,
        pinned_box,
        config,
        state,
        pinned_file,
        on_launch,
        status_label,
    );
    let flow = ui::app_grid::build_app_flow_box(
        config,
        state,
        Some(category_ids),
        "",
        pinned_file,
        Rc::clone(on_launch),
        status_label,
        Some(&on_rebuild),
    );
    flow.set_halign(gtk4::Align::Center);
    well.append(&flow);

    // Install grid navigation linked to pinned section above
    let pinned_flow = pinned_box
        .first_child()
        .and_then(|w| w.downcast::<gtk4::FlowBox>().ok());
    ui::navigation::install_grid_nav(&flow, config.columns, pinned_flow.as_ref(), None);
    // Re-link pinned's Down target to this new flow
    if let Some(ref pf) = pinned_flow {
        let pinned = state.borrow().pinned.clone();
        let pinned_cols = config.columns.min(pinned.len() as u32).max(1);
        ui::navigation::install_grid_nav(pf, pinned_cols, None, Some(&flow));
    }
}

/// Updates CSS classes so only the clicked button has "category-selected".
fn select_button(active: &gtk4::Button, buttons: &Rc<RefCell<Vec<gtk4::Button>>>) {
    for btn in buttons.borrow().iter() {
        btn.remove_css_class("category-selected");
    }
    active.add_css_class("category-selected");
}
