use super::ipc::{read_response_with_type, send_message, MSG_SUBSCRIBE};
use crate::compositor::traits::WmEventStream;
use crate::compositor::types::WmEvent;
use crate::error::DockError;
use std::os::unix::net::UnixStream;

/// i3-ipc event type for output events (bit 31 set + type 7).
const EVENT_OUTPUT: u32 = 0x80000007;

pub(super) struct SwayEventStream {
    conn: UnixStream,
}

impl SwayEventStream {
    /// Subscribes to window and output events, taking ownership of the connection.
    pub(super) fn connect(mut conn: UnixStream) -> crate::error::Result<Self> {
        // Subscribe to window and output events (output for monitor hotplug)
        let payload = b"[\"window\",\"output\"]";
        send_message(&mut conn, MSG_SUBSCRIBE, payload)?;
        let reply = super::ipc::read_response(&mut conn)?;
        // Check subscription success
        let result: serde_json::Value = serde_json::from_slice(&reply)?;
        if result.get("success").and_then(|v| v.as_bool()) != Some(true) {
            return Err(DockError::Ipc(std::io::Error::other(
                "Sway event subscription failed",
            )));
        }
        Ok(Self { conn })
    }
}

impl WmEventStream for SwayEventStream {
    fn next_event(&mut self) -> std::result::Result<WmEvent, std::io::Error> {
        loop {
            let (body, msg_type) =
                read_response_with_type(&mut self.conn).map_err(|e| match e {
                    DockError::Ipc(io) => io,
                    other => std::io::Error::other(other.to_string()),
                })?;

            // Output events → MonitorChanged
            if msg_type == EVENT_OUTPUT {
                return Ok(WmEvent::MonitorChanged);
            }

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
