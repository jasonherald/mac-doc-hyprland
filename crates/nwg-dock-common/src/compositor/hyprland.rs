use super::traits::{Compositor, WmEventStream};
use super::types::{WmClient, WmEvent, WmMonitor, WmWorkspace};
use crate::error::Result;
use crate::hyprland::events::{EventStream, HyprEvent};
use crate::hyprland::ipc;
use crate::hyprland::types::{HyprClient, HyprMonitor};

/// Hyprland compositor backend.
pub struct HyprlandBackend;

impl HyprlandBackend {
    pub fn new() -> Result<Self> {
        ipc::instance_signature()?;
        Ok(Self)
    }
}

impl Compositor for HyprlandBackend {
    fn list_clients(&self) -> Result<Vec<WmClient>> {
        Ok(ipc::list_clients()?.into_iter().map(to_wm_client).collect())
    }

    fn list_monitors(&self) -> Result<Vec<WmMonitor>> {
        Ok(ipc::list_monitors()?
            .into_iter()
            .map(to_wm_monitor)
            .collect())
    }

    fn get_active_window(&self) -> Result<WmClient> {
        Ok(to_wm_client(ipc::get_active_window()?))
    }

    fn get_cursor_position(&self) -> Option<(i32, i32)> {
        let reply = match ipc::hyprctl("j/cursorpos") {
            Ok(r) => r,
            Err(e) => {
                log::debug!("Failed to get cursor position: {}", e);
                return None;
            }
        };
        let val: serde_json::Value = match serde_json::from_slice(&reply) {
            Ok(v) => v,
            Err(e) => {
                log::debug!("Failed to parse cursor position: {}", e);
                return None;
            }
        };
        let x = val.get("x")?.as_i64()? as i32;
        let y = val.get("y")?.as_i64()? as i32;
        Some((x, y))
    }

    fn focus_window(&self, id: &str) -> Result<()> {
        ipc::hyprctl(&format!("dispatch focuswindow address:{}", id))?;
        Ok(())
    }

    fn close_window(&self, id: &str) -> Result<()> {
        ipc::hyprctl(&format!("dispatch closewindow address:{}", id))?;
        Ok(())
    }

    fn toggle_floating(&self, id: &str) -> Result<()> {
        ipc::hyprctl(&format!("dispatch togglefloating address:{}", id))?;
        Ok(())
    }

    fn toggle_fullscreen(&self, id: &str) -> Result<()> {
        ipc::hyprctl(&format!("dispatch fullscreen address:{}", id))?;
        Ok(())
    }

    fn move_to_workspace(&self, id: &str, workspace: i32) -> Result<()> {
        ipc::hyprctl(&format!(
            "dispatch movetoworkspace {},address:{}",
            workspace, id
        ))?;
        Ok(())
    }

    fn toggle_special_workspace(&self, name: &str) -> Result<()> {
        ipc::hyprctl(&format!("dispatch togglespecialworkspace {}", name))?;
        Ok(())
    }

    fn raise_active(&self) -> Result<()> {
        ipc::hyprctl("dispatch bringactivetotop")?;
        Ok(())
    }

    fn exec(&self, cmd: &str) -> Result<()> {
        let sanitized = super::sanitize_exec_command(cmd);
        ipc::hyprctl(&format!("dispatch exec {}", sanitized))?;
        Ok(())
    }

    fn event_stream(&self) -> Result<Box<dyn WmEventStream>> {
        Ok(Box::new(HyprlandEventStream(EventStream::connect()?)))
    }

    fn supports_cursor_position(&self) -> bool {
        true
    }
}

struct HyprlandEventStream(EventStream);

impl WmEventStream for HyprlandEventStream {
    fn next_event(&mut self) -> std::result::Result<WmEvent, std::io::Error> {
        match self.0.next_event()? {
            HyprEvent::ActiveWindowV2(addr) => Ok(WmEvent::ActiveWindowChanged(addr)),
            HyprEvent::MonitorChanged => Ok(WmEvent::MonitorChanged),
            HyprEvent::Other(s) => Ok(WmEvent::Other(s)),
        }
    }
}

/// Converts a Hyprland client to a compositor-neutral WmClient.
///
/// Fields intentionally not carried over (not needed by dock/drawer UI):
/// mapped, hidden, at, size, initial_title, xwayland,
/// pinned, fake_fullscreen, fullscreen_mode, grouped, swallowing.
fn to_wm_client(c: HyprClient) -> WmClient {
    WmClient {
        id: c.address,
        class: c.class,
        initial_class: c.initial_class,
        title: c.title,
        pid: c.pid,
        workspace: WmWorkspace {
            id: c.workspace.id,
            name: c.workspace.name,
        },
        floating: c.floating,
        monitor_id: c.monitor,
        fullscreen: c.fullscreen != 0,
    }
}

/// Converts a Hyprland monitor to a compositor-neutral WmMonitor.
///
/// Hyprland's `j/monitors` reports native pixel dimensions (before transform
/// or scale), but cursor coordinates from `j/cursorpos` use the logical layout
/// space (after transform and scale). Convert to logical dimensions so bounds
/// checking in the cursor poller works correctly for rotated and scaled monitors.
fn to_wm_monitor(m: HyprMonitor) -> WmMonitor {
    // Transforms 1,3,5,7 swap width↔height (90°, 270°, and their flipped variants)
    let (pw, ph) = if matches!(m.transform, 1 | 3 | 5 | 7) {
        (m.height, m.width)
    } else {
        (m.width, m.height)
    };
    let scale = if m.scale > 0.0 { m.scale } else { 1.0 };
    WmMonitor {
        id: m.id,
        name: m.name,
        width: (pw as f64 / scale).round() as i32,
        height: (ph as f64 / scale).round() as i32,
        x: m.x,
        y: m.y,
        scale: m.scale,
        focused: m.focused,
        active_workspace: WmWorkspace {
            id: m.active_workspace.id,
            name: m.active_workspace.name,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyprland::types::WorkspaceRef;

    fn test_hypr_monitor(w: i32, h: i32, scale: f64, transform: i32) -> HyprMonitor {
        HyprMonitor {
            width: w,
            height: h,
            scale,
            transform,
            name: "DP-1".into(),
            active_workspace: WorkspaceRef::default(),
            ..Default::default()
        }
    }

    #[test]
    fn to_wm_monitor_no_transform_no_scale() {
        let wm = to_wm_monitor(test_hypr_monitor(2560, 1440, 1.0, 0));
        assert_eq!(wm.width, 2560);
        assert_eq!(wm.height, 1440);
    }

    #[test]
    fn to_wm_monitor_scaled() {
        // 4K at 1.5x scale → logical 2560×1440
        let wm = to_wm_monitor(test_hypr_monitor(3840, 2160, 1.5, 0));
        assert_eq!(wm.width, 2560);
        assert_eq!(wm.height, 1440);
    }

    #[test]
    fn to_wm_monitor_rotated_90() {
        // 2560×1600 native, rotated 90° → logical 1600×2560
        let wm = to_wm_monitor(test_hypr_monitor(2560, 1600, 1.0, 1));
        assert_eq!(wm.width, 1600);
        assert_eq!(wm.height, 2560);
    }

    #[test]
    fn to_wm_monitor_rotated_270() {
        let wm = to_wm_monitor(test_hypr_monitor(2560, 1600, 1.0, 3));
        assert_eq!(wm.width, 1600);
        assert_eq!(wm.height, 2560);
    }

    #[test]
    fn to_wm_monitor_rotated_and_scaled() {
        // 3840×2160 native, rotated 90°, scale 1.5 → logical 1440×2560
        let wm = to_wm_monitor(test_hypr_monitor(3840, 2160, 1.5, 1));
        assert_eq!(wm.width, 1440);
        assert_eq!(wm.height, 2560);
    }

    #[test]
    fn to_wm_monitor_180_no_swap() {
        // 180° rotation doesn't swap dimensions
        let wm = to_wm_monitor(test_hypr_monitor(2560, 1440, 1.0, 2));
        assert_eq!(wm.width, 2560);
        assert_eq!(wm.height, 1440);
    }
}
