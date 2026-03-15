use dock_common::desktop::entry::DesktopEntry;
use std::collections::HashMap;
use std::path::PathBuf;

/// Mutable state for the drawer application.
#[allow(dead_code)]
pub struct DrawerState {
    /// All parsed desktop entries (id → entry).
    pub id2entry: HashMap<String, DesktopEntry>,

    /// Desktop entries sorted by name for display.
    pub desktop_entries: Vec<DesktopEntry>,

    /// Category lists: category_name → vec of desktop IDs.
    pub category_lists: HashMap<String, Vec<String>>,

    /// Pinned item desktop IDs.
    pub pinned: Vec<String>,

    /// App directories.
    pub app_dirs: Vec<PathBuf>,

    /// Current search phrase.
    pub search_phrase: String,

    /// XDG user directory map (e.g. "documents" → "/home/user/Documents").
    pub user_dirs: HashMap<String, PathBuf>,

    /// Directories excluded from file search.
    pub exclusions: Vec<String>,

    /// Whether a scroll happened (prevents accidental launch).
    pub been_scrolled: bool,
}

impl DrawerState {
    pub fn new(app_dirs: Vec<PathBuf>) -> Self {
        Self {
            id2entry: HashMap::new(),
            desktop_entries: Vec::new(),
            category_lists: HashMap::new(),
            pinned: Vec::new(),
            app_dirs,
            search_phrase: String::new(),
            user_dirs: map_xdg_user_dirs(),
            exclusions: Vec::new(),
            been_scrolled: false,
        }
    }
}

/// Maps XDG user directory names to paths.
fn map_xdg_user_dirs() -> HashMap<String, PathBuf> {
    let mut result = HashMap::new();
    let home = std::env::var("HOME").unwrap_or_default();

    result.insert("home".into(), PathBuf::from(&home));
    result.insert("documents".into(), PathBuf::from(&home).join("Documents"));
    result.insert("downloads".into(), PathBuf::from(&home).join("Downloads"));
    result.insert("music".into(), PathBuf::from(&home).join("Music"));
    result.insert("pictures".into(), PathBuf::from(&home).join("Pictures"));
    result.insert("videos".into(), PathBuf::from(&home).join("Videos"));

    // Try to read XDG user-dirs.dirs config
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| format!("{}/.config", home));
    let user_dirs_file = PathBuf::from(&config_home).join("user-dirs.dirs");

    if let Ok(content) = std::fs::read_to_string(&user_dirs_file) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(val) = parse_user_dir_line(line, &home) {
                if line.starts_with("XDG_DOCUMENTS_DIR") {
                    result.insert("documents".into(), val);
                } else if line.starts_with("XDG_DOWNLOAD_DIR") {
                    result.insert("downloads".into(), val);
                } else if line.starts_with("XDG_MUSIC_DIR") {
                    result.insert("music".into(), val);
                } else if line.starts_with("XDG_PICTURES_DIR") {
                    result.insert("pictures".into(), val);
                } else if line.starts_with("XDG_VIDEOS_DIR") {
                    result.insert("videos".into(), val);
                }
            }
        }
    }

    result
}

/// Parses a line like `XDG_DOCUMENTS_DIR="$HOME/Documents"` into a PathBuf.
fn parse_user_dir_line(line: &str, home: &str) -> Option<PathBuf> {
    let (_, value) = line.split_once('=')?;
    let value = value.trim().trim_matches('"');
    let expanded = value.replace("$HOME", home);
    Some(PathBuf::from(expanded))
}
