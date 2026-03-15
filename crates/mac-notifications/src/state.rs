use crate::notification::{Notification, Urgency};
use gtk4::gio;
use std::collections::HashSet;
use std::path::PathBuf;

/// A group of notifications from the same application.
pub struct AppGroup {
    pub app_name: String,
    pub app_icon: String,
    pub notifications: Vec<Notification>,
}

/// Mutable state for the notification daemon.
pub struct NotificationState {
    /// All notifications, newest first.
    pub history: Vec<Notification>,
    /// Next ID to assign (starts at 1).
    next_id: u32,
    /// Do Not Disturb mode.
    pub dnd: bool,
    /// When timed DND expires (None = permanent or off).
    pub dnd_expires: Option<std::time::SystemTime>,
    /// App directories for icon resolution.
    pub app_dirs: Vec<PathBuf>,
    /// IDs of notifications currently showing as popups.
    pub active_popups: HashSet<u32>,
    /// Maximum history entries to retain.
    pub max_history: usize,
    /// D-Bus connection for emitting ActionInvoked signals.
    pub dbus_connection: Option<gio::DBusConnection>,
}

impl NotificationState {
    pub fn new(app_dirs: Vec<PathBuf>, max_history: usize) -> Self {
        Self {
            history: Vec::new(),
            next_id: 1,
            dnd: false,
            dnd_expires: None,
            app_dirs,
            active_popups: HashSet::new(),
            max_history,
            dbus_connection: None,
        }
    }

    /// Adds a notification and returns its assigned ID.
    pub fn add(&mut self, mut notif: Notification) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        notif.id = id;
        self.history.insert(0, notif);
        self.trim_history();
        id
    }

    /// Replaces an existing notification or adds a new one.
    pub fn replace(&mut self, replaces_id: u32, mut notif: Notification) -> u32 {
        if replaces_id > 0
            && let Some(existing) = self.history.iter_mut().find(|n| n.id == replaces_id)
        {
            notif.id = replaces_id;
            *existing = notif;
            return replaces_id;
        }
        self.add(notif)
    }

    /// Removes a notification by ID.
    pub fn remove(&mut self, id: u32) -> Option<Notification> {
        if let Some(pos) = self.history.iter().position(|n| n.id == id) {
            self.active_popups.remove(&id);
            Some(self.history.remove(pos))
        } else {
            None
        }
    }

    /// Dismisses all notifications from a specific app.
    pub fn dismiss_app(&mut self, app_name: &str) {
        self.history.retain(|n| n.app_name != app_name);
    }

    /// Clears all notifications.
    pub fn dismiss_all(&mut self) {
        self.history.clear();
        self.active_popups.clear();
    }

    /// Marks a notification as read.
    pub fn mark_read(&mut self, id: u32) {
        if let Some(notif) = self.history.iter_mut().find(|n| n.id == id) {
            notif.read = true;
        }
    }

    /// Returns the count of unread notifications.
    pub fn unread_count(&self) -> usize {
        self.history.iter().filter(|n| !n.read).count()
    }

    /// Whether a popup should be shown for this urgency given current DND state.
    pub fn should_show_popup(&self, urgency: Urgency) -> bool {
        if !self.dnd {
            return true;
        }
        // In DND, only critical notifications show popups
        urgency == Urgency::Critical
    }

    /// Groups notifications by app, preserving insertion order.
    pub fn grouped_by_app(&self) -> Vec<AppGroup> {
        let mut index: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        let mut groups: Vec<AppGroup> = Vec::new();

        for notif in &self.history {
            if let Some(&idx) = index.get(notif.app_name.as_str()) {
                groups[idx].notifications.push(notif.clone());
            } else {
                index.insert(&notif.app_name, groups.len());
                groups.push(AppGroup {
                    app_name: notif.app_name.clone(),
                    app_icon: notif.app_icon.clone(),
                    notifications: vec![notif.clone()],
                });
            }
        }

        groups
    }

    fn trim_history(&mut self) {
        if self.history.len() > self.max_history {
            self.history.truncate(self.max_history);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn test_notif(app: &str, summary: &str) -> Notification {
        Notification {
            id: 0,
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
    fn add_assigns_sequential_ids() {
        let mut state = NotificationState::new(vec![], 100);
        let id1 = state.add(test_notif("app1", "first"));
        let id2 = state.add(test_notif("app2", "second"));
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    fn replace_reuses_id() {
        let mut state = NotificationState::new(vec![], 100);
        let id = state.add(test_notif("app", "original"));
        let replaced = state.replace(id, test_notif("app", "updated"));
        assert_eq!(replaced, id);
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].summary, "updated");
    }

    #[test]
    fn dismiss_app_removes_only_matching() {
        let mut state = NotificationState::new(vec![], 100);
        state.add(test_notif("firefox", "tab1"));
        state.add(test_notif("discord", "msg1"));
        state.add(test_notif("firefox", "tab2"));
        state.dismiss_app("firefox");
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].app_name, "discord");
    }

    #[test]
    fn unread_count() {
        let mut state = NotificationState::new(vec![], 100);
        let id1 = state.add(test_notif("app", "one"));
        state.add(test_notif("app", "two"));
        assert_eq!(state.unread_count(), 2);
        state.mark_read(id1);
        assert_eq!(state.unread_count(), 1);
    }

    #[test]
    fn grouped_by_app_groups_correctly() {
        let mut state = NotificationState::new(vec![], 100);
        state.add(test_notif("firefox", "tab1"));
        state.add(test_notif("discord", "msg"));
        state.add(test_notif("firefox", "tab2"));
        let groups = state.grouped_by_app();
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn dnd_suppresses_normal_popups() {
        let mut state = NotificationState::new(vec![], 100);
        state.dnd = true;
        assert!(!state.should_show_popup(Urgency::Normal));
        assert!(!state.should_show_popup(Urgency::Low));
        assert!(state.should_show_popup(Urgency::Critical));
    }

    #[test]
    fn trim_history_caps_at_max() {
        let mut state = NotificationState::new(vec![], 3);
        state.add(test_notif("app", "1"));
        state.add(test_notif("app", "2"));
        state.add(test_notif("app", "3"));
        state.add(test_notif("app", "4"));
        assert_eq!(state.history.len(), 3);
    }
}
