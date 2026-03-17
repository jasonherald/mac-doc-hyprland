use clap::{Parser, ValueEnum};

/// Popup display position corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PopupPosition {
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

/// A macOS-style notification daemon for Hyprland/Sway.
#[derive(Parser, Debug, Clone)]
#[command(name = "nwg-notifications", version, about)]
pub struct NotificationConfig {
    /// Popup display position
    #[arg(long, value_enum, default_value_t = PopupPosition::TopRight)]
    pub popup_position: PopupPosition,

    /// Default popup timeout in ms (macOS uses ~7 seconds)
    #[arg(long, default_value_t = 7000)]
    pub popup_timeout: u64,

    /// Maximum simultaneous popups
    #[arg(long, default_value_t = 5)]
    pub max_popups: usize,

    /// Maximum history entries to retain
    #[arg(long, default_value_t = 200)]
    pub max_history: usize,

    /// Start in Do Not Disturb mode
    #[arg(long)]
    pub dnd: bool,

    /// Persist notification history across restarts
    #[arg(long)]
    pub persist: bool,

    /// Turn on debug messages
    #[arg(long)]
    pub debug: bool,

    /// Window manager override (auto-detected from environment if not specified)
    #[arg(long, default_value = "")]
    pub wm: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let config = NotificationConfig::parse_from(["test"]);
        assert_eq!(config.popup_position, PopupPosition::TopRight);
        assert_eq!(config.popup_timeout, 7000);
        assert_eq!(config.max_history, 200);
        assert!(!config.dnd);
    }

    #[test]
    fn dnd_flag() {
        let config = NotificationConfig::parse_from(["test", "--dnd"]);
        assert!(config.dnd);
    }
}
