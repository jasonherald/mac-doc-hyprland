use crate::config::DrawerConfig;
use crate::state::DrawerState;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Performs file search across XDG user directories and populates the flow box.
pub fn search_files(
    phrase: &str,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    on_launch: Rc<dyn Fn()>,
) -> gtk4::FlowBox {
    let flow_box = gtk4::FlowBox::new();
    flow_box.set_selection_mode(gtk4::SelectionMode::None);

    let user_dirs = state.borrow().user_dirs.clone();
    let exclusions = state.borrow().exclusions.clone();
    let preferred_apps = state.borrow().preferred_apps.clone();

    for (dir_name, dir_path) in &user_dirs {
        if dir_name == "home" {
            continue;
        }
        if !dir_path.exists() {
            continue;
        }

        let results = walk_directory(dir_path, phrase, &exclusions);
        if results.is_empty() {
            continue;
        }

        // Add directory header button
        let header = dir_header_button(dir_name, dir_path, Rc::clone(&on_launch));
        flow_box.insert(&header, -1);

        // Add result buttons
        for result in &results {
            let is_dir = result.is_dir;
            let display = result
                .path
                .strip_prefix(dir_path)
                .unwrap_or(&result.path)
                .to_string_lossy()
                .to_string();

            let btn = file_result_button(
                &display,
                &result.path,
                is_dir,
                config,
                &preferred_apps,
                Rc::clone(&on_launch),
            );
            flow_box.insert(&btn, -1);
        }
    }

    flow_box
}

struct FileResult {
    path: std::path::PathBuf,
    is_dir: bool,
}

/// Recursively walks a directory, returning matching file paths.
fn walk_directory(
    root: &Path,
    phrase: &str,
    exclusions: &[String],
) -> Vec<FileResult> {
    let mut results = Vec::new();
    let phrase_lower = phrase.to_lowercase();

    fn walk_inner(
        dir: &Path,
        root: &Path,
        phrase: &str,
        exclusions: &[String],
        results: &mut Vec<FileResult>,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Check exclusions
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy();
            if exclusions.iter().any(|ex| relative.contains(ex)) {
                continue;
            }

            // Search relative path (not just filename) — matches Go's behavior
            if relative.to_lowercase().contains(phrase) {
                results.push(FileResult {
                    is_dir: path.is_dir(),
                    path: path.clone(),
                });
            }

            if path.is_dir() {
                walk_inner(&path, root, phrase, exclusions, results);
            }
        }
    }

    walk_inner(root, root, &phrase_lower, exclusions, &mut results);
    results
}

fn dir_header_button(
    dir_name: &str,
    dir_path: &Path,
    on_launch: Rc<dyn Fn()>,
) -> gtk4::Button {
    let button = gtk4::Button::with_label(dir_name);
    let path = dir_path.to_path_buf();
    let on_launch = Rc::clone(&on_launch);
    button.connect_clicked(move |_| {
        let _ = std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn();
        on_launch();
    });
    button
}

fn file_result_button(
    display_name: &str,
    file_path: &Path,
    is_dir: bool,
    config: &DrawerConfig,
    preferred_apps: &std::collections::HashMap<String, String>,
    on_launch: Rc<dyn Fn()>,
) -> gtk4::Button {
    let label = if display_name.len() > config.fs_name_limit {
        format!("{}…", &display_name[..config.fs_name_limit.saturating_sub(1)])
    } else {
        display_name.to_string()
    };

    let button = gtk4::Button::with_label(&label);
    if display_name.len() > config.fs_name_limit {
        button.set_tooltip_text(Some(display_name));
    }

    if is_dir {
        button.add_css_class("file-search-dir");
    }

    // Check for preferred app override
    let path = file_path.to_path_buf();
    let path_str = file_path.to_string_lossy().to_string();
    let preferred_cmd = dock_common::desktop::preferred_apps::find_preferred_app(
        &path_str,
        preferred_apps,
    );

    button.connect_clicked(move |_| {
        if let Some(ref cmd) = preferred_cmd {
            let _ = std::process::Command::new(cmd).arg(&path).spawn();
        } else {
            let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
        }
        on_launch();
    });

    button
}
