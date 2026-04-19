use serde::Deserialize;

/// Reference to a workspace (embedded in Client and Monitor).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct WorkspaceRef {
    pub id: i32,
    pub name: String,
}

/// A Hyprland window/client.
///
/// Uses `serde(default)` to handle field differences across Hyprland versions.
/// Newer Hyprland versions may add/remove fields (e.g. fullscreenMode removed,
/// fullscreenClient added).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HyprClient {
    pub address: String,
    pub mapped: bool,
    pub hidden: bool,
    pub at: Vec<i32>,
    pub size: Vec<i32>,
    pub workspace: WorkspaceRef,
    pub floating: bool,
    pub monitor: i32,
    pub class: String,
    pub title: String,
    pub initial_class: String,
    pub initial_title: String,
    pub pid: i32,
    pub xwayland: bool,
    pub pinned: bool,
    pub fullscreen: i32,
    pub fullscreen_mode: i32,
    pub fake_fullscreen: bool,
    pub grouped: Vec<serde_json::Value>,
    pub swallowing: serde_json::Value,
}

/// A Hyprland monitor/output.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HyprMonitor {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub make: String,
    pub model: String,
    pub serial: String,
    pub width: i32,
    pub height: i32,
    pub refresh_rate: f64,
    pub x: i32,
    pub y: i32,
    pub active_workspace: WorkspaceRef,
    pub reserved: Vec<i32>,
    pub scale: f64,
    pub transform: i32,
    pub focused: bool,
    pub dpms_status: bool,
    pub vrr: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_client() {
        let json = r#"{
            "address": "0x1234",
            "mapped": true,
            "hidden": false,
            "at": [100, 200],
            "size": [800, 600],
            "workspace": {"id": 1, "name": "1"},
            "floating": false,
            "monitor": 0,
            "class": "firefox",
            "title": "Mozilla Firefox",
            "initialClass": "firefox",
            "initialTitle": "Firefox",
            "pid": 12345,
            "xwayland": false,
            "pinned": false,
            "fullscreen": 0,
            "fullscreenMode": 0,
            "fakeFullscreen": false,
            "grouped": [],
            "swallowing": "0x0"
        }"#;
        let client: HyprClient = serde_json::from_str(json).unwrap();
        assert_eq!(client.class, "firefox");
        assert_eq!(client.workspace.id, 1);
        assert_eq!(client.pid, 12345);
    }

    #[test]
    fn deserialize_monitor() {
        let json = r#"{
            "id": 0,
            "name": "DP-1",
            "description": "Some Monitor",
            "make": "Dell",
            "model": "U2720Q",
            "serial": "ABC123",
            "width": 3840,
            "height": 2160,
            "refreshRate": 60.0,
            "x": 0,
            "y": 0,
            "activeWorkspace": {"id": 1, "name": "1"},
            "reserved": [0, 0, 0, 0],
            "scale": 1.5,
            "transform": 0,
            "focused": true,
            "dpmsStatus": true,
            "vrr": false
        }"#;
        let monitor: HyprMonitor = serde_json::from_str(json).unwrap();
        assert_eq!(monitor.name, "DP-1");
        assert_eq!(monitor.width, 3840);
        assert!((monitor.scale - 1.5).abs() < f64::EPSILON);
    }
}
