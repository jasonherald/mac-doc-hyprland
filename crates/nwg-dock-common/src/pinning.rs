use std::fs;
use std::path::Path;

/// Checks if a task ID is in the pinned list.
pub fn is_pinned(pinned: &[String], task_id: &str) -> bool {
    pinned.iter().any(|p| p.trim() == task_id.trim())
}

/// Adds an item to the pinned list if not already present.
/// Returns true if the item was added.
pub fn pin_item(pinned: &mut Vec<String>, item_id: &str) -> bool {
    if is_pinned(pinned, item_id) {
        log::debug!("{} already pinned", item_id);
        return false;
    }
    pinned.push(item_id.to_string());
    true
}

/// Removes an item from the pinned list.
/// Returns true if the item was removed.
pub fn unpin_item(pinned: &mut Vec<String>, item_id: &str) -> bool {
    let len = pinned.len();
    pinned.retain(|p| p.trim() != item_id.trim());
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
        Err(_) => Vec::new(),
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
}
