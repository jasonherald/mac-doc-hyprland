use crate::compositor::types::{WmClient, WmMonitor, WmWorkspace};

/// Converts a Sway container node into a WmClient.
pub(super) fn node_to_wm_client(node: &serde_json::Value, floating: bool) -> Option<WmClient> {
    let id = node.get("id")?.as_i64()?.to_string();

    // Wayland: app_id, X11: window_properties.class
    let class = node
        .get("app_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            node.get("window_properties")
                .and_then(|p| p.get("class"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string();

    let title = node
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let pid = node.get("pid").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

    let fullscreen_mode = node
        .get("fullscreen_mode")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let (ws_id, ws_name) = extract_workspace_from_node(node);

    Some(WmClient {
        id,
        class,
        initial_class: String::new(), // Sway doesn't track initial class
        title,
        pid,
        workspace: WmWorkspace {
            id: ws_id,
            name: ws_name,
        },
        floating,
        monitor_id: 0, // Set during tree traversal
        fullscreen: fullscreen_mode > 0,
    })
}

/// Attempts to extract workspace info. Sway nodes don't directly embed
/// their workspace, so we rely on the node having a `workspace` field
/// (available in event containers) or default to 0/"".
fn extract_workspace_from_node(node: &serde_json::Value) -> (i32, String) {
    // In some contexts (e.g., event container), workspace info may be embedded
    if let Some(ws) = node.get("workspace") {
        let id = ws.get("num").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let name = ws
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return (id, name);
    }
    (0, String::new())
}

/// Converts a Sway output JSON node into a WmMonitor.
pub(super) fn output_to_wm_monitor(output: &serde_json::Value, idx: i32) -> WmMonitor {
    let rect = output.get("rect").cloned().unwrap_or_default();
    let current_mode = output.get("current_mode").cloned().unwrap_or_default();

    let width = current_mode
        .get("width")
        .and_then(|v| v.as_i64())
        .or_else(|| rect.get("width").and_then(|v| v.as_i64()))
        .unwrap_or(0) as i32;
    let height = current_mode
        .get("height")
        .and_then(|v| v.as_i64())
        .or_else(|| rect.get("height").and_then(|v| v.as_i64()))
        .unwrap_or(0) as i32;

    WmMonitor {
        id: idx,
        name: output
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        width,
        height,
        x: rect.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        y: rect.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        scale: output.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0),
        focused: output
            .get("focused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        active_workspace: {
            let ws_name = output
                .get("current_workspace")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            WmWorkspace {
                id: 0,
                name: ws_name,
            }
        },
    }
}
