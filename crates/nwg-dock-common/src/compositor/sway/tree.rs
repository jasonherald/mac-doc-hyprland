use super::types::{node_to_wm_client, output_to_wm_monitor};
use crate::compositor::types::{WmClient, WmMonitor, WmWorkspace};

/// Maximum tree traversal depth to prevent stack overflow.
const MAX_TREE_DEPTH: u32 = 128;

/// A node is a window if it has a pid and either app_id (Wayland) or
/// window_properties (X11).
pub(super) fn is_window_node(node: &serde_json::Value) -> bool {
    let has_pid = node.get("pid").and_then(|v| v.as_i64()).unwrap_or(0) > 0;
    let has_app_id = node
        .get("app_id")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty());
    let has_window_props = node.get("window_properties").is_some();
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
    has_pid && (has_app_id || has_window_props) && node_type == "con"
}

/// Finds the focused window in the tree by recursively searching for
/// the deepest node with `focused: true`.
pub(super) fn find_focused_window(node: &serde_json::Value) -> Option<WmClient> {
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

/// Collects windows with proper workspace information by traversing
/// the tree depth-first, tracking the current workspace and output context.
pub(super) fn collect_windows_with_context(
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

/// Parses Sway outputs list into monitor list, filtering active only.
pub(super) fn parse_monitors(reply: &[u8]) -> crate::error::Result<Vec<WmMonitor>> {
    let outputs: Vec<serde_json::Value> = serde_json::from_slice(reply)?;
    Ok(outputs
        .into_iter()
        .filter(|o| o.get("active").and_then(|v| v.as_bool()) == Some(true))
        .enumerate()
        .map(|(i, o)| output_to_wm_monitor(&o, i as i32))
        .collect())
}

/// Collects all client windows from the tree root, matching list_clients logic.
pub(super) fn collect_clients_from_tree(tree: &serde_json::Value) -> Vec<WmClient> {
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
                collect_windows_with_context(child, &mut clients, &default_ws, output_idx, false);
                output_idx += 1;
            }
        }
    }
    clients
}
