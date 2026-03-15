use std::collections::HashMap;
use std::path::Path;

/// Loads preferred app associations from a JSON file.
///
/// The file maps regex patterns to commands, e.g.:
/// `{ ".*\\.pdf$": "zathura", ".*\\.mp4$": "mpv" }`
pub fn load_preferred_apps(path: &Path) -> Option<HashMap<String, String>> {
    let content = std::fs::read_to_string(path).ok()?;
    let map: HashMap<String, serde_json::Value> = serde_json::from_str(&content).ok()?;
    let result: HashMap<String, String> = map
        .into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect();
    if result.is_empty() { None } else { Some(result) }
}

/// Finds the preferred command for a file path by matching patterns.
/// Uses simple glob-style matching (contains check) since full regex
/// would require an additional dependency.
pub fn find_preferred_app(
    file_path: &str,
    preferred_apps: &HashMap<String, String>,
) -> Option<String> {
    let file_lower = file_path.to_lowercase();
    for (pattern, command) in preferred_apps {
        // Support simple suffix patterns like "*.pdf" or ".pdf"
        let pattern = pattern.trim_start_matches('*');
        if file_lower.ends_with(&pattern.to_lowercase()) {
            return Some(command.clone());
        }
    }
    None
}
