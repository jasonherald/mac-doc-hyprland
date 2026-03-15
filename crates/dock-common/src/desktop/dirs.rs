use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Returns all XDG application directories, including flatpak locations.
pub fn get_app_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    let home = env::var("HOME").unwrap_or_default();
    let xdg_data_home = env::var("XDG_DATA_HOME").ok();
    let xdg_data_dirs = env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share/:/usr/share/".to_string());

    // User data dir first
    if let Some(ref data_home) = xdg_data_home {
        dirs.push(PathBuf::from(data_home).join("applications"));
    } else if !home.is_empty() {
        dirs.push(PathBuf::from(&home).join(".local/share/applications"));
    }

    // System data dirs
    for dir in xdg_data_dirs.split(':') {
        let app_dir = PathBuf::from(dir).join("applications");
        if !dirs.contains(&app_dir) {
            dirs.push(app_dir);
        }
    }

    // Flatpak dirs
    let flatpak_dirs = [
        PathBuf::from(&home).join(".local/share/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];
    for dir in &flatpak_dirs {
        if !dirs.contains(dir) {
            dirs.push(dir.clone());
        }
    }

    dirs
}

/// Lists all .desktop files in the given directory (non-recursive).
pub fn list_desktop_files(dir: &Path) -> Vec<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext == "desktop")
        })
        .collect()
}

/// Searches desktop directories for a .desktop file matching the app name.
///
/// Handles cases where the window class doesn't exactly match the .desktop filename,
/// e.g. "gimp-2.9.9" matching "gimp.desktop" or "org.gimp.GIMP.desktop".
pub fn search_desktop_dirs(app_id: &str, app_dirs: &[PathBuf]) -> Option<PathBuf> {
    let before_dash = app_id.split('-').next().unwrap_or(app_id).to_string();
    let before_space = app_id.split(' ').next().unwrap_or(app_id).to_string();
    let app_id = app_id.to_string();
    let app_upper = app_id.to_uppercase();
    let space_upper = before_space.to_uppercase();
    let wm_class_needle = format!("StartupWMClass={}", before_space);

    // Multi-pass search with decreasing specificity
    type MatchFn = Box<dyn Fn(&str, &Path) -> bool>;
    let desktop_suffix = format!("{}.desktop", app_id);
    let desktop_upper = format!("{}.DESKTOP", app_upper);

    let searches: Vec<MatchFn> = vec![
        // 1. org.*.appid.desktop pattern
        Box::new(move |name: &str, _: &Path| {
            name.contains(&*before_dash)
                && name.matches('.').count() > 1
                && name.ends_with(&desktop_suffix)
        }),
        // 2. Exact case-insensitive match
        Box::new(move |name: &str, _: &Path| {
            name.to_uppercase() == desktop_upper
        }),
        // 3. Contains app name (case-insensitive)
        Box::new(move |name: &str, _: &Path| {
            name.to_uppercase().contains(&space_upper)
        }),
        // 4. StartupWMClass in file contents
        Box::new(move |_: &str, path: &Path| {
            fs::read_to_string(path).is_ok_and(|c| c.contains(&wm_class_needle))
        }),
    ];

    for search in &searches {
        for dir in app_dirs {
            for entry in fs::read_dir(dir).into_iter().flatten().filter_map(|e| e.ok()) {
                if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                    continue;
                }
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if search(&name_str, &entry.path()) {
                    return Some(entry.path());
                }
            }
        }
    }

    None
}
