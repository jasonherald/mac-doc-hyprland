use crate::notification::Notification;
use std::path::{Path, PathBuf};

/// Returns the path to the notification history file.
pub fn history_path() -> PathBuf {
    dock_common::config::paths::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("mac-notifications-history.json")
}

/// Loads notification history from disk.
pub fn load_history(path: &Path) -> Vec<Notification> {
    match std::fs::read_to_string(path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
            log::warn!("Failed to parse notification history: {}", e);
            Vec::new()
        }),
        Err(_) => Vec::new(),
    }
}

/// Saves notification history to disk.
pub fn save_history(path: &Path, history: &[Notification]) {
    match serde_json::to_string(history) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                log::error!("Failed to save notification history: {}", e);
            }
        }
        Err(e) => log::error!("Failed to serialize notification history: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::Urgency;
    use std::time::SystemTime;

    fn test_notif(app: &str, summary: &str) -> Notification {
        Notification {
            id: 1,
            app_name: app.into(),
            app_icon: String::new(),
            summary: summary.into(),
            body: String::new(),
            actions: Vec::new(),
            urgency: Urgency::Normal,
            timeout_ms: -1,
            timestamp: SystemTime::now(),
            read: false,
            desktop_entry: None,
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir();
        let path = dir.join("mac-notif-test-history.json");

        let history = vec![
            test_notif("firefox", "New tab"),
            test_notif("discord", "Message from user"),
        ];

        save_history(&path, &history);
        let loaded = load_history(&path);

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].app_name, "firefox");
        assert_eq!(loaded[1].summary, "Message from user");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let loaded = load_history(Path::new("/tmp/nonexistent-mac-notif-test.json"));
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_corrupt_json_returns_empty() {
        let path = std::env::temp_dir().join("mac-notif-test-corrupt.json");
        std::fs::write(&path, "not valid json {{{").ok();

        let loaded = load_history(&path);
        assert!(loaded.is_empty());

        std::fs::remove_file(&path).ok();
    }
}
