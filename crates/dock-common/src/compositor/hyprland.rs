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
        let reply = ipc::hyprctl("j/cursorpos").ok()?;
        let val: serde_json::Value = serde_json::from_slice(&reply).ok()?;
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
        ipc::hyprctl(&format!("dispatch exec {}", cmd))?;
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
            HyprEvent::Other(s) => Ok(WmEvent::Other(s)),
        }
    }
}

fn to_wm_client(c: HyprClient) -> WmClient {
    WmClient {
        id: c.address,
        class: c.class,
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

fn to_wm_monitor(m: HyprMonitor) -> WmMonitor {
    WmMonitor {
        id: m.id,
        name: m.name,
        width: m.width,
        height: m.height,
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
