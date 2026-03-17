use crate::state::DrawerState;
use nwg_dock_common::desktop::categories::assign_categories;
use nwg_dock_common::desktop::dirs;
use nwg_dock_common::desktop::entry;

/// Scans all app directories, parses .desktop files, and populates state.
pub fn load_desktop_entries(state: &mut DrawerState) {
    state.apps.id2entry.clear();
    state.apps.entries.clear();
    state.apps.category_lists.clear();

    let mut seen_ids = std::collections::HashSet::new();

    for dir in &state.app_dirs {
        let files = dirs::list_desktop_files(dir);
        for path in files {
            let id = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // First occurrence wins (matches Go behavior)
            if seen_ids.contains(&id) {
                continue;
            }
            seen_ids.insert(id.clone());

            match entry::parse_desktop_file(&id, &path) {
                Ok(de) => {
                    if !de.no_display {
                        // Assign to ALL matching categories (matches Go behavior)
                        for cat in assign_categories(&de.category) {
                            state
                                .apps
                                .category_lists
                                .entry(cat.to_string())
                                .or_default()
                                .push(de.desktop_id.clone());
                        }
                    }
                    state
                        .apps
                        .id2entry
                        .insert(de.desktop_id.clone(), de.clone());
                    state.apps.entries.push(de);
                }
                Err(e) => {
                    log::warn!("Failed to parse {}: {}", path.display(), e);
                }
            }
        }
    }

    // Sort by localized name
    state
        .apps
        .entries
        .sort_by_key(|a| a.name_loc.to_lowercase());

    log::info!(
        "Loaded {} desktop entries from {} directories",
        state.apps.entries.len(),
        state.app_dirs.len()
    );
}
