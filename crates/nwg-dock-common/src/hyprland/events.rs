use crate::error::Result;
use crate::hyprland::ipc;
use std::io::Read;
use std::os::unix::net::UnixStream;

/// Events emitted by the Hyprland event stream.
#[derive(Debug, Clone)]
pub enum HyprEvent {
    /// Active window changed. Contains the window address.
    ActiveWindowV2(String),
    /// Monitor added or removed.
    MonitorChanged,
    /// Any other event we don't specifically handle.
    Other(String),
}

/// Maximum event line buffer size (64KB) to prevent OOM from a misbehaving socket.
const MAX_EVENT_BUFFER: usize = 65536;

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
    ///
    /// Returns `Ok(event)` on success, `Err` on socket error, `Ok(None)` style
    /// isn't used — instead the outer Option distinguishes EOF from data.
    /// Returns `None` only on clean connection close (EOF).
    pub fn next_event(&mut self) -> std::result::Result<HyprEvent, std::io::Error> {
        loop {
            if let Some(newline_pos) = self.remainder.find('\n') {
                let line = self.remainder[..newline_pos].to_string();
                self.remainder = self.remainder[newline_pos + 1..].to_string();
                if !line.is_empty() {
                    return Ok(parse_event(&line));
                }
                continue;
            }

            if self.remainder.len() > MAX_EVENT_BUFFER {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "event line too long (exceeds 64KB)",
                ));
            }

            let n = self.conn.read(&mut self.buf)?;
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "Hyprland event socket closed",
                ));
            }
            let chunk = String::from_utf8_lossy(&self.buf[..n]);
            self.remainder.push_str(&chunk);
        }
    }
}

fn parse_event(line: &str) -> HyprEvent {
    if let Some(addr) = line.strip_prefix("activewindowv2>>") {
        HyprEvent::ActiveWindowV2(addr.trim().to_string())
    } else if line.starts_with("monitoraddedv2>>") || line.starts_with("monitorremoved>>") {
        HyprEvent::MonitorChanged
    } else {
        HyprEvent::Other(line.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_active_window() {
        match parse_event("activewindowv2>>0x5678abcd") {
            HyprEvent::ActiveWindowV2(addr) => assert_eq!(addr, "0x5678abcd"),
            other => panic!("expected ActiveWindowV2, got {:?}", other),
        }
    }

    #[test]
    fn parse_monitor_added() {
        assert!(matches!(
            parse_event("monitoraddedv2>>DP-1,1920x1080@60"),
            HyprEvent::MonitorChanged
        ));
    }

    #[test]
    fn parse_monitor_removed() {
        assert!(matches!(
            parse_event("monitorremoved>>HDMI-A-1"),
            HyprEvent::MonitorChanged
        ));
    }

    #[test]
    fn parse_other_event() {
        assert!(matches!(
            parse_event("workspace>>2"),
            HyprEvent::Other(_)
        ));
    }
}
