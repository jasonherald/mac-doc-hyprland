use crate::error::Result;
use crate::hyprland::ipc;
use std::io::Read;
use std::os::unix::net::UnixStream;

/// Events emitted by the Hyprland event stream.
#[derive(Debug, Clone)]
pub enum HyprEvent {
    /// A client changed in a way that may affect the visible client list:
    /// focus moved, a window opened/closed, or a window moved across
    /// workspaces. Carries the address from the originating event so
    /// downstream dedup against the last-seen address still works.
    ///
    /// Folded together so the dock has a single "rebuild may be needed"
    /// signal — `needs_rebuild` does the actual class-list diff and
    /// short-circuits if nothing changed. This mirrors what the Sway
    /// backend already does (`new`/`close`/`focus` → ActiveWindowChanged).
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
    } else if let Some(rest) = line
        .strip_prefix("openwindow>>")
        .or_else(|| line.strip_prefix("closewindow>>"))
        .or_else(|| line.strip_prefix("movewindowv2>>"))
        .or_else(|| line.strip_prefix("movewindow>>"))
    {
        // First comma-delimited field is the window address for all four events.
        let addr = rest.split(',').next().unwrap_or("").trim().to_string();
        HyprEvent::ActiveWindowV2(addr)
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
        assert!(matches!(parse_event("workspace>>2"), HyprEvent::Other(_)));
    }

    /// Regression: closing a non-focused window must propagate as a window-change
    /// signal, not get swallowed as `Other`. Without this the dock keeps showing
    /// a button for an app that is no longer running.
    #[test]
    fn parse_close_window_yields_address() {
        match parse_event("closewindow>>0xdeadbeef") {
            HyprEvent::ActiveWindowV2(addr) => assert_eq!(addr, "0xdeadbeef"),
            other => panic!("expected ActiveWindowV2, got {:?}", other),
        }
    }

    /// Companion to the close case — a new window must trigger the same signal.
    #[test]
    fn parse_open_window_extracts_first_field() {
        match parse_event("openwindow>>0xabc123,1,Alacritty,Terminal") {
            HyprEvent::ActiveWindowV2(addr) => assert_eq!(addr, "0xabc123"),
            other => panic!("expected ActiveWindowV2, got {:?}", other),
        }
    }

    /// movewindow events change which workspace a client is on, which can
    /// affect which clients should appear in the dock under workspace filters.
    #[test]
    fn parse_move_window_extracts_address() {
        match parse_event("movewindow>>0xfeed,2") {
            HyprEvent::ActiveWindowV2(addr) => assert_eq!(addr, "0xfeed"),
            other => panic!("expected ActiveWindowV2, got {:?}", other),
        }
    }

    #[test]
    fn parse_movewindow_v2_extracts_address() {
        match parse_event("movewindowv2>>0xfeed,3,workspace-3") {
            HyprEvent::ActiveWindowV2(addr) => assert_eq!(addr, "0xfeed"),
            other => panic!("expected ActiveWindowV2, got {:?}", other),
        }
    }
}
