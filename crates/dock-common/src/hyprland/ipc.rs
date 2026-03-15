use crate::error::{DockError, Result};
use crate::hyprland::types::{HyprClient, HyprMonitor};
use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

/// Returns the Hyprland socket directory.
fn hypr_socket_dir() -> Result<PathBuf> {
    let his = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| DockError::EnvNotSet("HYPRLAND_INSTANCE_SIGNATURE".into()))?;

    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    let hypr_dir = if !runtime_dir.is_empty() {
        let candidate = PathBuf::from(&runtime_dir).join("hypr");
        if candidate.exists() { candidate } else { PathBuf::from("/tmp/hypr") }
    } else {
        PathBuf::from("/tmp/hypr")
    };

    Ok(hypr_dir.join(his))
}

/// Returns the HYPRLAND_INSTANCE_SIGNATURE value.
pub fn instance_signature() -> Result<String> {
    env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| DockError::EnvNotSet("HYPRLAND_INSTANCE_SIGNATURE".into()))
}

/// Sends a command to Hyprland via the IPC socket and returns the raw response.
pub fn hyprctl(cmd: &str) -> Result<Vec<u8>> {
    let socket_path = hypr_socket_dir()?.join(".socket.sock");
    let mut conn = UnixStream::connect(&socket_path)?;
    conn.write_all(cmd.as_bytes())?;

    let mut reply = vec![0u8; 102400];
    let n = conn.read(&mut reply)?;
    reply.truncate(n);
    Ok(reply)
}

/// Lists all Hyprland clients (windows).
pub fn list_clients() -> Result<Vec<HyprClient>> {
    let reply = hyprctl("j/clients")?;
    let clients: Vec<HyprClient> = serde_json::from_slice(&reply)?;
    Ok(clients)
}

/// Lists all Hyprland monitors.
pub fn list_monitors() -> Result<Vec<HyprMonitor>> {
    let reply = hyprctl("j/monitors")?;
    let monitors: Vec<HyprMonitor> = serde_json::from_slice(&reply)?;
    Ok(monitors)
}

/// Gets the currently active window.
pub fn get_active_window() -> Result<HyprClient> {
    let reply = hyprctl("j/activewindow")?;
    let client: HyprClient = serde_json::from_slice(&reply)?;
    Ok(client)
}

/// Returns the path to the Hyprland event socket (socket2).
pub fn event_socket_path() -> Result<PathBuf> {
    Ok(hypr_socket_dir()?.join(".socket2.sock"))
}
