use crate::error::Result;
use crate::hyprland::ipc;
use std::io::Read;
use std::os::unix::net::UnixStream;

/// Events emitted by the Hyprland event stream.
#[derive(Debug, Clone)]
pub enum HyprEvent {
    /// Active window changed. Contains the window address.
    ActiveWindowV2(String),
    /// Any other event we don't specifically handle.
    Other(String),
}

/// Blocking event stream reader for Hyprland socket2.
///
/// Connects to Hyprland's event socket and yields events.
/// Designed to be run on a background thread.
pub struct EventStream {
    conn: UnixStream,
    buf: Vec<u8>,
    remainder: String,
}

impl EventStream {
    /// Connects to the Hyprland event socket.
    pub fn connect() -> Result<Self> {
        let path = ipc::event_socket_path()?;
        let conn = UnixStream::connect(path)?;
        Ok(Self {
            conn,
            buf: vec![0u8; 10240],
            remainder: String::new(),
        })
    }

    /// Blocks until the next event is available.
    /// Returns `None` on connection close.
    pub fn next_event(&mut self) -> Option<HyprEvent> {
        loop {
            // Check if we have a complete line in remainder
            if let Some(newline_pos) = self.remainder.find('\n') {
                let line = self.remainder[..newline_pos].to_string();
                self.remainder = self.remainder[newline_pos + 1..].to_string();
                if !line.is_empty() {
                    return Some(parse_event(&line));
                }
                continue;
            }

            // Read more data
            match self.conn.read(&mut self.buf) {
                Ok(0) => return None,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&self.buf[..n]);
                    self.remainder.push_str(&chunk);
                }
                Err(e) => {
                    log::error!("Error reading from Hyprland event socket: {}", e);
                    return None;
                }
            }
        }
    }
}

fn parse_event(line: &str) -> HyprEvent {
    if let Some(addr) = line.strip_prefix("activewindowv2>>") {
        HyprEvent::ActiveWindowV2(addr.trim().to_string())
    } else {
        HyprEvent::Other(line.to_string())
    }
}
