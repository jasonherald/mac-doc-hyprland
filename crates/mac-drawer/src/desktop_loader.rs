use crate::state::DrawerState;
use dock_common::desktop::categories::assign_category;
use dock_common::desktop::dirs;
use dock_common::desktop::entry;

/// Scans all app directories, parses .desktop files, and populates state.
pub fn load_desktop_entries(state: &mut DrawerState) {
    state.id2entry.clear();
    state.desktop_entries.clear();
    state.category_lists.clear();

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
                        // Assign to category
                        let cat = assign_category(&de.category).to_string();
                        state
                            .category_lists
                            .entry(cat)
                            .or_default()
                            .push(de.desktop_id.clone());
                    }
                    state.id2entry.insert(de.desktop_id.clone(), de.clone());
                    state.desktop_entries.push(de);
                }
                Err(e) => {
                    log::warn!("Failed to parse {}: {}", path.display(), e);
                }
            }
        }
    }

    // Sort by localized name
    state.desktop_entries.sort_by(|a, b| {
        a.name_loc
            .to_lowercase()
            .cmp(&b.name_loc.to_lowercase())
    });

    log::info!(
        "Loaded {} desktop entries from {} directories",
        state.desktop_entries.len(),
        state.app_dirs.len()
    );
}
