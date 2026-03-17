use super::traits::{Compositor, WmEventStream};
use super::types::{WmClient, WmEvent, WmMonitor, WmWorkspace};
use crate::error::{DockError, Result};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

// i3-ipc message types
const MSG_RUN_COMMAND: u32 = 0;
const MSG_SUBSCRIBE: u32 = 2;
const MSG_GET_OUTPUTS: u32 = 3;
const MSG_GET_TREE: u32 = 4;

const I3_IPC_MAGIC: &[u8; 6] = b"i3-ipc";
const HEADER_SIZE: usize = 14; // 6 (magic) + 4 (length) + 4 (type)

/// Sway compositor backend using the i3-compatible IPC protocol.
pub struct SwayBackend {
    socket_path: PathBuf,
}

impl SwayBackend {
    pub fn new() -> Result<Self> {
        let path =
            std::env::var("SWAYSOCK").map_err(|_| DockError::EnvNotSet("SWAYSOCK".into()))?;
        Ok(Self {
            socket_path: PathBuf::from(path),
        })
    }

    fn command(&self, msg_type: u32, payload: &[u8]) -> Result<Vec<u8>> {
        let mut conn = UnixStream::connect(&self.socket_path)?;
        send_message(&mut conn, msg_type, payload)?;
        read_response(&mut conn)
    }

    fn run_command(&self, cmd: &str) -> Result<()> {
        let reply = self.command(MSG_RUN_COMMAND, cmd.as_bytes())?;
        // Sway returns [{"success": true/false, ...}]
        let results: Vec<serde_json::Value> = serde_json::from_slice(&reply)?;
        if let Some(first) = results.first()
            && first.get("success").and_then(|v| v.as_bool()) == Some(false)
        {
            let err_msg = first
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(DockError::Ipc(std::io::Error::other(err_msg.to_string())));
        }
        Ok(())
    }
}

impl Compositor for SwayBackend {
    fn list_clients(&self) -> Result<Vec<WmClient>> {
        let reply = self.command(MSG_GET_TREE, &[])?;
        let tree: serde_json::Value = serde_json::from_slice(&reply)?;
        let mut clients = Vec::new();
        let default_ws = WmWorkspace {
            id: 0,
            name: String::new(),
        };
        // Enumerate root-level outputs to track monitor index
        let mut output_idx: i32 = 0;
        if let Some(nodes) = tree.get("nodes").and_then(|v| v.as_array()) {
            for child in nodes {
                let node_type = child.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let name = child.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if node_type == "output" && !name.starts_with("__") {
                    collect_windows_with_context(
                        child,
                        &mut clients,
                        &default_ws,
                        output_idx,
                        false,
                    );
                    output_idx += 1;
                }
            }
        }
        Ok(clients)
    }

    fn list_monitors(&self) -> Result<Vec<WmMonitor>> {
        let reply = self.command(MSG_GET_OUTPUTS, &[])?;
        let outputs: Vec<serde_json::Value> = serde_json::from_slice(&reply)?;
        Ok(outputs
            .into_iter()
            .filter(|o| o.get("active").and_then(|v| v.as_bool()) == Some(true))
            .enumerate()
            .map(|(i, o)| output_to_wm_monitor(&o, i as i32))
            .collect())
    }

    fn get_active_window(&self) -> Result<WmClient> {
        let reply = self.command(MSG_GET_TREE, &[])?;
        let tree: serde_json::Value = serde_json::from_slice(&reply)?;
        find_focused_window(&tree).ok_or_else(|| {
            DockError::Ipc(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no focused window",
            ))
        })
    }

    fn get_cursor_position(&self) -> Option<(i32, i32)> {
        // Sway does not expose cursor position via IPC
        None
    }

    fn focus_window(&self, id: &str) -> Result<()> {
        self.run_command(&format!("[con_id={}] focus", id))
    }

    fn close_window(&self, id: &str) -> Result<()> {
        self.run_command(&format!("[con_id={}] kill", id))
    }

    fn toggle_floating(&self, id: &str) -> Result<()> {
        self.run_command(&format!("[con_id={}] floating toggle", id))
    }

    fn toggle_fullscreen(&self, id: &str) -> Result<()> {
        self.run_command(&format!("[con_id={}] fullscreen toggle", id))
    }

    fn move_to_workspace(&self, id: &str, workspace: i32) -> Result<()> {
        self.run_command(&format!(
            "[con_id={}] move to workspace number {}",
            id, workspace
        ))
    }

    fn toggle_special_workspace(&self, _name: &str) -> Result<()> {
        // Sway's equivalent of special workspaces is the scratchpad
        self.run_command("scratchpad show")
    }

    fn raise_active(&self) -> Result<()> {
        // Sway manages its own stacking — no equivalent needed
        Ok(())
    }

    fn exec(&self, cmd: &str) -> Result<()> {
        let sanitized = super::sanitize_exec_command(cmd);
        self.run_command(&format!("exec {}", sanitized))
    }

    fn event_stream(&self) -> Result<Box<dyn WmEventStream>> {
        let mut conn = UnixStream::connect(&self.socket_path)?;
        // Subscribe to window events
        let payload = b"[\"window\"]";
        send_message(&mut conn, MSG_SUBSCRIBE, payload)?;
        let reply = read_response(&mut conn)?;
        // Check subscription success
        let result: serde_json::Value = serde_json::from_slice(&reply)?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            return Err(DockError::Ipc(std::io::Error::other(
                "Sway event subscription failed",
            )));
        }
        Ok(Box::new(SwayEventStream { conn }))
    }

    fn supports_cursor_position(&self) -> bool {
        false
    }
}

struct SwayEventStream {
    conn: UnixStream,
}

impl WmEventStream for SwayEventStream {
    fn next_event(&mut self) -> std::result::Result<WmEvent, std::io::Error> {
        loop {
            let body = read_response(&mut self.conn).map_err(|e| match e {
                DockError::Ipc(io) => io,
                other => std::io::Error::other(other.to_string()),
            })?;
            let event: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            let change = event.get("change").and_then(|v| v.as_str()).unwrap_or("");

            match change {
                "focus" | "new" | "close" => {
                    // Extract the container id as the window identifier
                    let id = event
                        .get("container")
                        .and_then(|c| c.get("id"))
                        .and_then(|v| v.as_i64())
                        .map(|id| id.to_string())
                        .unwrap_or_default();
                    return Ok(WmEvent::ActiveWindowChanged(id));
                }
                // Skip events like "title", "fullscreen_mode", "floating", etc.
                _ => continue,
            }
        }
    }
}

// --- i3-ipc protocol helpers ---

fn send_message(conn: &mut UnixStream, msg_type: u32, payload: &[u8]) -> Result<()> {
    let mut header = Vec::with_capacity(HEADER_SIZE + payload.len());
    header.extend_from_slice(I3_IPC_MAGIC);
    header.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    header.extend_from_slice(&msg_type.to_le_bytes());
    header.extend_from_slice(payload);
    conn.write_all(&header)?;
    Ok(())
}

/// Maximum IPC response payload size (100MB safety cap).
const MAX_PAYLOAD_SIZE: usize = 100_000_000;

fn read_response(conn: &mut UnixStream) -> Result<Vec<u8>> {
    let mut header = [0u8; HEADER_SIZE];
    conn.read_exact(&mut header)?;

    // Validate magic
    if &header[..6] != I3_IPC_MAGIC {
        return Err(DockError::Ipc(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid i3-ipc magic in response",
        )));
    }

    let payload_len =
        u32::from_le_bytes(header[6..10].try_into().expect("slice is exactly 4 bytes")) as usize;
    if payload_len > MAX_PAYLOAD_SIZE {
        return Err(DockError::Ipc(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("IPC payload too large: {} bytes", payload_len),
        )));
    }
    let mut body = vec![0u8; payload_len];
    conn.read_exact(&mut body)?;
    Ok(body)
}

// --- Tree traversal ---

/// A node is a window if it has a pid and either app_id (Wayland) or
/// window_properties (X11).
fn is_window_node(node: &serde_json::Value) -> bool {
    let has_pid = node.get("pid").and_then(|v| v.as_i64()).unwrap_or(0) > 0;
    let has_app_id = node
        .get("app_id")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty());
    let has_window_props = node.get("window_properties").is_some();
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
    has_pid && (has_app_id || has_window_props) && node_type == "con"
}

fn node_to_wm_client(node: &serde_json::Value, floating: bool) -> Option<WmClient> {
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

/// Finds the focused window in the tree by recursively searching for
/// the deepest node with `focused: true`.
fn find_focused_window(node: &serde_json::Value) -> Option<WmClient> {
    find_focused_window_inner(node, false)
}

fn find_focused_window_inner(node: &serde_json::Value, is_floating: bool) -> Option<WmClient> {
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let floating = is_floating || node_type == "floating_con";

    if node.get("focused").and_then(|v| v.as_bool()) == Some(true) && is_window_node(node) {
        return node_to_wm_client(node, floating);
    }

    if let Some(nodes) = node.get("nodes").and_then(|v| v.as_array()) {
        for child in nodes {
            if let Some(found) = find_focused_window_inner(child, floating) {
                return Some(found);
            }
        }
    }
    if let Some(floating_nodes) = node.get("floating_nodes").and_then(|v| v.as_array()) {
        for child in floating_nodes {
            if let Some(found) = find_focused_window_inner(child, true) {
                return Some(found);
            }
        }
    }

    None
}

fn output_to_wm_monitor(output: &serde_json::Value, idx: i32) -> WmMonitor {
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

// --- Enhanced tree traversal with workspace context ---

/// Maximum tree traversal depth to prevent stack overflow.
const MAX_TREE_DEPTH: u32 = 128;

/// Collects windows with proper workspace information by traversing
/// the tree depth-first, tracking the current workspace and output context.
fn collect_windows_with_context(
    node: &serde_json::Value,
    windows: &mut Vec<WmClient>,
    current_workspace: &WmWorkspace,
    current_output: i32,
    is_floating: bool,
) {
    collect_windows_recursive(
        node,
        windows,
        current_workspace,
        current_output,
        is_floating,
        0,
    );
}

fn collect_windows_recursive(
    node: &serde_json::Value,
    windows: &mut Vec<WmClient>,
    current_workspace: &WmWorkspace,
    current_output: i32,
    is_floating: bool,
    depth: u32,
) {
    if depth > MAX_TREE_DEPTH {
        log::warn!("Sway tree traversal depth limit exceeded");
        return;
    }

    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match node_type {
        "workspace" => {
            let ws_name = node
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // Skip Sway's internal __i3_scratch workspace
            if ws_name.starts_with("__") {
                return;
            }
            let ws_num = node.get("num").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let ws = WmWorkspace {
                id: ws_num,
                name: ws_name,
            };
            recurse_children(node, windows, &ws, current_output, false, depth);
            return;
        }
        "output" => {
            let name = node.get("name").and_then(|v| v.as_str()).unwrap_or("");
            // Skip Sway's internal __i3 output
            if name.starts_with("__") {
                return;
            }
            recurse_children(
                node,
                windows,
                current_workspace,
                current_output,
                false,
                depth,
            );
            return;
        }
        // floating_con is the container that wraps floating windows
        "floating_con" => {
            recurse_children(
                node,
                windows,
                current_workspace,
                current_output,
                true,
                depth,
            );
            return;
        }
        _ => {}
    }

    if is_window_node(node)
        && let Some(mut client) = node_to_wm_client(node, is_floating)
    {
        client.workspace = current_workspace.clone();
        client.monitor_id = current_output;
        windows.push(client);
    }

    recurse_children(
        node,
        windows,
        current_workspace,
        current_output,
        is_floating,
        depth,
    );
}

fn recurse_children(
    node: &serde_json::Value,
    windows: &mut Vec<WmClient>,
    ws: &WmWorkspace,
    output: i32,
    is_floating: bool,
    depth: u32,
) {
    if let Some(nodes) = node.get("nodes").and_then(|v| v.as_array()) {
        for child in nodes {
            collect_windows_recursive(child, windows, ws, output, is_floating, depth + 1);
        }
    }
    if let Some(floating) = node.get("floating_nodes").and_then(|v| v.as_array()) {
        for child in floating {
            // Children of floating_nodes are floating
            collect_windows_recursive(child, windows, ws, output, true, depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_header() {
        let mut buf = Vec::new();
        buf.extend_from_slice(I3_IPC_MAGIC);
        buf.extend_from_slice(&5u32.to_le_bytes());
        buf.extend_from_slice(&MSG_RUN_COMMAND.to_le_bytes());
        assert_eq!(&buf[..6], b"i3-ipc");
        assert_eq!(u32::from_le_bytes(buf[6..10].try_into().unwrap()), 5);
        assert_eq!(u32::from_le_bytes(buf[10..14].try_into().unwrap()), 0);
    }

    #[test]
    fn collect_windows_from_tree() {
        let tree = serde_json::json!({
            "type": "root",
            "nodes": [{
                "type": "output",
                "name": "DP-1",
                "nodes": [{
                    "type": "workspace",
                    "name": "1",
                    "num": 1,
                    "nodes": [{
                        "type": "con",
                        "id": 42,
                        "name": "Firefox",
                        "app_id": "firefox",
                        "pid": 1234,
                        "focused": false,
                        "fullscreen_mode": 0,
                        "nodes": [],
                        "floating_nodes": []
                    }],
                    "floating_nodes": [{
                        "type": "floating_con",
                        "nodes": [{
                            "type": "con",
                            "id": 43,
                            "name": "Calculator",
                            "app_id": "gnome-calculator",
                            "pid": 5678,
                            "focused": false,
                            "fullscreen_mode": 0,
                            "nodes": [],
                            "floating_nodes": []
                        }],
                        "floating_nodes": []
                    }]
                }]
            }],
            "floating_nodes": []
        });

        let mut clients = Vec::new();
        let ws = WmWorkspace {
            id: 0,
            name: String::new(),
        };
        collect_windows_with_context(&tree, &mut clients, &ws, 0, false);
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].class, "firefox");
        assert_eq!(clients[0].id, "42");
        assert_eq!(clients[0].title, "Firefox");
        assert_eq!(clients[0].pid, 1234);
        assert_eq!(clients[0].workspace.name, "1");
        assert_eq!(clients[0].workspace.id, 1);
        assert!(!clients[0].floating);
        assert_eq!(clients[1].class, "gnome-calculator");
        assert_eq!(clients[1].id, "43");
        assert!(clients[1].floating); // nested under floating_nodes
    }

    #[test]
    fn find_focused_in_tree() {
        let tree = serde_json::json!({
            "type": "root",
            "focused": false,
            "nodes": [{
                "type": "output",
                "focused": false,
                "nodes": [{
                    "type": "workspace",
                    "focused": false,
                    "nodes": [
                        {
                            "type": "con",
                            "id": 10,
                            "name": "Unfocused",
                            "app_id": "app1",
                            "pid": 100,
                            "focused": false,
                            "fullscreen_mode": 0,
                            "nodes": [],
                            "floating_nodes": []
                        },
                        {
                            "type": "con",
                            "id": 20,
                            "name": "Focused Window",
                            "app_id": "app2",
                            "pid": 200,
                            "focused": true,
                            "fullscreen_mode": 0,
                            "nodes": [],
                            "floating_nodes": []
                        }
                    ],
                    "floating_nodes": []
                }]
            }],
            "floating_nodes": []
        });

        let focused = find_focused_window(&tree);
        assert!(focused.is_some());
        let f = focused.unwrap();
        assert_eq!(f.id, "20");
        assert_eq!(f.class, "app2");
        assert_eq!(f.title, "Focused Window");
    }

    #[test]
    fn x11_window_uses_window_properties() {
        let node = serde_json::json!({
            "type": "con",
            "id": 99,
            "name": "Steam",
            "app_id": null,
            "pid": 9999,
            "focused": false,
            "fullscreen_mode": 0,
            "window_properties": {
                "class": "steam",
                "instance": "steam",
                "title": "Steam"
            },
            "nodes": [],
            "floating_nodes": []
        });

        let client = node_to_wm_client(&node, false).unwrap();
        assert_eq!(client.class, "steam");
        assert_eq!(client.id, "99");
    }

    #[test]
    fn parse_output() {
        let output = serde_json::json!({
            "name": "DP-1",
            "active": true,
            "focused": true,
            "scale": 1.5,
            "current_workspace": "1",
            "rect": {"x": 0, "y": 0, "width": 3840, "height": 2160},
            "current_mode": {"width": 3840, "height": 2160, "refresh": 60000}
        });

        let mon = output_to_wm_monitor(&output, 0);
        assert_eq!(mon.name, "DP-1");
        assert_eq!(mon.width, 3840);
        assert_eq!(mon.height, 2160);
        assert!(mon.focused);
        assert!((mon.scale - 1.5).abs() < f64::EPSILON);
        assert_eq!(mon.active_workspace.name, "1");
    }

    #[test]
    fn parse_event_json() {
        let event = serde_json::json!({
            "change": "focus",
            "container": {
                "id": 42,
                "name": "Firefox",
                "app_id": "firefox",
                "focused": true
            }
        });

        let change = event.get("change").and_then(|v| v.as_str()).unwrap();
        assert_eq!(change, "focus");
        let id = event
            .get("container")
            .and_then(|c| c.get("id"))
            .and_then(|v| v.as_i64())
            .unwrap();
        assert_eq!(id, 42);
    }

    #[test]
    fn empty_tree_yields_no_windows() {
        let tree = serde_json::json!({
            "type": "root",
            "nodes": [],
            "floating_nodes": []
        });
        let mut clients = Vec::new();
        let ws = WmWorkspace {
            id: 0,
            name: String::new(),
        };
        collect_windows_with_context(&tree, &mut clients, &ws, 0, false);
        assert!(clients.is_empty());
    }

    #[test]
    fn multi_monitor_assigns_correct_output_ids() {
        let tree = serde_json::json!({
            "type": "root",
            "nodes": [
                {
                    "type": "output",
                    "name": "__i3",
                    "nodes": [],
                    "floating_nodes": []
                },
                {
                    "type": "output",
                    "name": "DP-1",
                    "nodes": [{
                        "type": "workspace",
                        "name": "1",
                        "num": 1,
                        "nodes": [{
                            "type": "con",
                            "id": 10,
                            "name": "App on DP-1",
                            "app_id": "app1",
                            "pid": 100,
                            "focused": false,
                            "fullscreen_mode": 0,
                            "nodes": [],
                            "floating_nodes": []
                        }],
                        "floating_nodes": []
                    }],
                    "floating_nodes": []
                },
                {
                    "type": "output",
                    "name": "HDMI-1",
                    "nodes": [{
                        "type": "workspace",
                        "name": "2",
                        "num": 2,
                        "nodes": [{
                            "type": "con",
                            "id": 20,
                            "name": "App on HDMI-1",
                            "app_id": "app2",
                            "pid": 200,
                            "focused": false,
                            "fullscreen_mode": 0,
                            "nodes": [],
                            "floating_nodes": []
                        }],
                        "floating_nodes": []
                    }],
                    "floating_nodes": []
                }
            ],
            "floating_nodes": []
        });

        // Simulate list_clients() root-level enumeration
        let mut clients = Vec::new();
        let default_ws = WmWorkspace {
            id: 0,
            name: String::new(),
        };
        let mut output_idx: i32 = 0;
        if let Some(nodes) = tree.get("nodes").and_then(|v| v.as_array()) {
            for child in nodes {
                let node_type = child.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let name = child.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if node_type == "output" && !name.starts_with("__") {
                    collect_windows_with_context(
                        child,
                        &mut clients,
                        &default_ws,
                        output_idx,
                        false,
                    );
                    output_idx += 1;
                }
            }
        }

        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].class, "app1");
        assert_eq!(clients[0].monitor_id, 0); // DP-1 = first real output
        assert_eq!(clients[0].workspace.name, "1");
        assert_eq!(clients[1].class, "app2");
        assert_eq!(clients[1].monitor_id, 1); // HDMI-1 = second real output
        assert_eq!(clients[1].workspace.name, "2");
    }

    #[test]
    fn no_focused_window_returns_none() {
        let tree = serde_json::json!({
            "type": "root",
            "focused": false,
            "nodes": [{
                "type": "con",
                "id": 1,
                "app_id": "test",
                "pid": 1,
                "focused": false,
                "fullscreen_mode": 0,
                "nodes": [],
                "floating_nodes": []
            }],
            "floating_nodes": []
        });
        assert!(find_focused_window(&tree).is_none());
    }
}
