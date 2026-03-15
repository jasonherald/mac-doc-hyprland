use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Urgency level per freedesktop notification specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Urgency {
    Low = 0,
    Normal = 1,
    Critical = 2,
}

impl From<u8> for Urgency {
    fn from(val: u8) -> Self {
        match val {
            0 => Urgency::Low,
            2 => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }
}

/// A single notification received via D-Bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub urgency: Urgency,
    pub timeout_ms: i32,
    pub timestamp: SystemTime,
    pub read: bool,
    pub desktop_entry: Option<String>,
}

/// Parses the flat actions array from D-Bus into (key, label) pairs.
/// D-Bus format: ["action-id-1", "Label 1", "action-id-2", "Label 2"]
pub fn parse_actions(flat: &[String]) -> Vec<(String, String)> {
    flat.chunks(2)
        .filter_map(|chunk| {
            if chunk.len() == 2 {
                Some((chunk[0].clone(), chunk[1].clone()))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urgency_from_u8() {
        assert_eq!(Urgency::from(0), Urgency::Low);
        assert_eq!(Urgency::from(1), Urgency::Normal);
        assert_eq!(Urgency::from(2), Urgency::Critical);
        assert_eq!(Urgency::from(255), Urgency::Normal);
    }

    #[test]
    fn parse_actions_pairs() {
        let flat = vec![
            "reply".into(),
            "Reply".into(),
            "dismiss".into(),
            "Dismiss".into(),
        ];
        let actions = parse_actions(&flat);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0], ("reply".into(), "Reply".into()));
    }

    #[test]
    fn parse_actions_odd_length() {
        let flat = vec!["only-one".into()];
        let actions = parse_actions(&flat);
        assert!(actions.is_empty());
    }
}
