use std::fs;
use std::path::Path;

/// Checks if a task ID is in the pinned list (case-insensitive).
pub fn is_pinned(pinned: &[String], task_id: &str) -> bool {
    let task_id = task_id.trim();
    pinned
        .iter()
        .any(|p| p.trim().eq_ignore_ascii_case(task_id))
}

/// Adds an item to the pinned list if not already present (case-insensitive).
/// Returns true if the item was added.
pub fn pin_item(pinned: &mut Vec<String>, item_id: &str) -> bool {
    let item_id = item_id.trim();
    if is_pinned(pinned, item_id) {
        log::debug!("{} already pinned", item_id);
        return false;
    }
    pinned.push(item_id.to_string());
    true
}

/// Removes an item from the pinned list (case-insensitive).
/// Returns true if the item was removed.
pub fn unpin_item(pinned: &mut Vec<String>, item_id: &str) -> bool {
    let item_id = item_id.trim();
    let len = pinned.len();
    pinned.retain(|p| !p.trim().eq_ignore_ascii_case(item_id));
    pinned.len() < len
}

/// Saves the pinned list to a file (one item per line).
///
/// Uses atomic write (temp file + rename) to prevent corruption on crash.
pub fn save_pinned(pinned: &[String], path: &Path) -> std::io::Result<()> {
    let content: String = pinned
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| format!("{}\n", line))
        .collect();
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, content)?;
    fs::rename(&temp_path, path)
}

/// Loads the pinned list from a file.
pub fn load_pinned(path: &Path) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(content) => content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("Failed to load pinned items from {}: {}", path.display(), e);
            }
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_unpin_roundtrip() {
        let mut pinned = Vec::new();
        assert!(pin_item(&mut pinned, "firefox"));
        assert!(!pin_item(&mut pinned, "firefox")); // already pinned
        assert!(is_pinned(&pinned, "firefox"));
        assert!(unpin_item(&mut pinned, "firefox"));
        assert!(!is_pinned(&pinned, "firefox"));
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join("dock-common-test-pinning");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test-pinned");

        let pinned = vec!["firefox".to_string(), "alacritty".to_string()];
        save_pinned(&pinned, &path).unwrap();

        let loaded = load_pinned(&path);
        assert_eq!(loaded, pinned);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn empty_file_loads_empty() {
        let dir = std::env::temp_dir().join("dock-common-test-pinning-empty");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test-pinned-empty");

        let pinned: Vec<String> = vec![];
        save_pinned(&pinned, &path).unwrap();

        let loaded = load_pinned(&path);
        assert!(loaded.is_empty());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn whitespace_only_lines_filtered() {
        let dir = std::env::temp_dir().join("dock-common-test-pinning-ws");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test-pinned-ws");

        // Write a file with blank lines and whitespace-only lines mixed in.
        std::fs::write(&path, "firefox\n\n   \nalacritty\n  \ngimp\n").unwrap();

        let loaded = load_pinned(&path);
        assert_eq!(loaded, vec!["firefox", "alacritty", "gimp"]);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn duplicate_pin_rejected() {
        let mut pinned = Vec::new();
        assert!(pin_item(&mut pinned, "firefox"));
        assert!(!pin_item(&mut pinned, "firefox"));
        assert_eq!(pinned.len(), 1);
    }

    #[test]
    fn large_pin_list_roundtrip() {
        let dir = std::env::temp_dir().join("dock-common-test-pinning-large");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test-pinned-large");

        let pinned: Vec<String> = (0..500).map(|i| format!("app-{}", i)).collect();
        save_pinned(&pinned, &path).unwrap();

        let loaded = load_pinned(&path);
        assert_eq!(loaded.len(), 500);
        assert_eq!(loaded, pinned);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn is_pinned_case_insensitive() {
        let pinned = vec!["slack".to_string()];
        assert!(is_pinned(&pinned, "Slack"));
        assert!(is_pinned(&pinned, "SLACK"));
        assert!(is_pinned(&pinned, "slack"));
    }

    #[test]
    fn unpin_case_insensitive() {
        let mut pinned = vec!["slack".to_string(), "firefox".to_string()];
        assert!(unpin_item(&mut pinned, "Slack"));
        assert!(!is_pinned(&pinned, "slack"));
        assert_eq!(pinned.len(), 1);
    }

    #[test]
    fn pin_rejects_case_insensitive_duplicate() {
        let mut pinned = Vec::new();
        assert!(pin_item(&mut pinned, "slack"));
        assert!(!pin_item(&mut pinned, "Slack"));
        assert_eq!(pinned.len(), 1);
    }

    #[test]
    fn load_nonexistent_file_returns_empty() {
        let path = std::env::temp_dir().join("dock-common-test-pinning-nonexistent-file-xyz");
        // Ensure the file does not exist.
        let _ = std::fs::remove_file(&path);
        let loaded = load_pinned(&path);
        assert!(loaded.is_empty());
    }
}
