use serde::Serialize;
use std::path::PathBuf;

/// Waybar refresh signal: SIGRTMIN+11 = 34+11 = 45.
const WAYBAR_REFRESH_SIGNAL: i32 = 45;

#[derive(Serialize)]
struct WaybarStatus {
    text: String,
    tooltip: String,
    alt: String,
    class: String,
}

/// Returns the path to the waybar status file.
fn status_path() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join("mac-notifications-status.json")
}

/// Writes the waybar status file and signals waybar to refresh.
pub fn update_status(unread: usize, dnd: bool) {
    let status = if dnd {
        WaybarStatus {
            text: "\u{f06d9}".into(), // 󰛙 bell-off
            tooltip: "Do Not Disturb".into(),
            alt: "dnd".into(),
            class: "dnd".into(),
        }
    } else if unread > 0 {
        WaybarStatus {
            text: format!("\u{f009a} {unread}"), // 󰂚 bell-badge + count
            tooltip: format!(
                "{unread} unread notification{}",
                if unread == 1 { "" } else { "s" }
            ),
            alt: "unread".into(),
            class: "unread".into(),
        }
    } else {
        WaybarStatus {
            text: "\u{f009c}".into(), // 󰂜 bell-outline
            tooltip: "No notifications".into(),
            alt: "empty".into(),
            class: "empty".into(),
        }
    };

    let path = status_path();
    match serde_json::to_string(&status) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::error!("Failed to write waybar status: {}", e);
            }
        }
        Err(e) => log::error!("Failed to serialize waybar status: {}", e),
    }

    signal_waybar();
}

/// Sends SIGRTMIN+11 to waybar to refresh the notification module.
fn signal_waybar() {
    match std::process::Command::new("pkill")
        .arg(format!("-{WAYBAR_REFRESH_SIGNAL}"))
        .arg("waybar")
        .status()
    {
        Err(e) => log::debug!("Failed to signal waybar: {e}"),
        Ok(s) if !s.success() => log::debug!("No waybar process to signal"),
        _ => {}
    }
}
