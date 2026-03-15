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
    let before_dash = app_id.split('-').next().unwrap_or(app_id);
    let before_space = app_id.split(' ').next().unwrap_or(app_id);

    // First pass: look for org.*.appid.desktop style matches
    for dir in app_dirs {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.contains(before_dash)
                && name_str.matches('.').count() > 1
                && name_str.ends_with(&format!("{}.desktop", app_id))
            {
                return Some(entry.path());
            }
        }
    }

    // Second pass: exact case-insensitive match
    for dir in app_dirs {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name_upper = name.to_string_lossy().to_uppercase();
            if name_upper == format!("{}.DESKTOP", app_id.to_uppercase()) {
                return Some(entry.path());
            }
        }
    }

    // Third pass: contains before_space (case-insensitive)
    for dir in app_dirs {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            if name
                .to_string_lossy()
                .to_uppercase()
                .contains(&before_space.to_uppercase())
            {
                return Some(entry.path());
            }
        }
    }

    // Fourth pass: check StartupWMClass in file contents
    for dir in app_dirs {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                continue;
            }
            let path = entry.path();
            if let Ok(content) = fs::read_to_string(&path)
                && content.contains(&format!("StartupWMClass={}", before_space)) {
                    return Some(path);
                }
        }
    }

    None
}
