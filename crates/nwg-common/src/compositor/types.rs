/// Compositor-neutral window representation.
///
/// Marked `#[non_exhaustive]` so additive field changes don't break
/// downstream match / struct-literal sites. External consumers construct
/// via [`WmClient::default`] plus the `with_*` setters below; same-crate
/// usage is unaffected by the attribute.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct WmClient {
    /// Compositor-specific identifier (Hyprland: `0x1234`, Sway: `42`).
    pub id: String,
    /// Application class (Hyprland: `class`; Sway: `app_id` or
    /// `window_properties.class`).
    pub class: String,
    /// Initial class at window creation (Hyprland only). Used to group child
    /// windows with their parent app (e.g. Playwright browsers under VSCode).
    /// Empty on backends that don't track this separately.
    pub initial_class: String,
    /// Human-readable window title.
    pub title: String,
    /// Process ID of the window's owning process, or 0 if unavailable.
    pub pid: i32,
    /// Workspace this window lives on.
    pub workspace: WmWorkspace,
    /// Whether the window is floating (not tiled).
    pub floating: bool,
    /// ID of the monitor this window is on. Matches [`WmMonitor::id`].
    pub monitor_id: i32,
    /// Whether the window is currently fullscreen.
    pub fullscreen: bool,
}

impl WmClient {
    /// Fluent setter for [`Self::id`].
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    /// Fluent setter for [`Self::class`].
    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        self.class = class.into();
        self
    }
    /// Fluent setter for [`Self::initial_class`].
    pub fn with_initial_class(mut self, initial_class: impl Into<String>) -> Self {
        self.initial_class = initial_class.into();
        self
    }
    /// Fluent setter for [`Self::title`].
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    /// Fluent setter for [`Self::pid`].
    pub fn with_pid(mut self, pid: i32) -> Self {
        self.pid = pid;
        self
    }
    /// Fluent setter for [`Self::workspace`].
    pub fn with_workspace(mut self, workspace: WmWorkspace) -> Self {
        self.workspace = workspace;
        self
    }
    /// Fluent setter for [`Self::floating`].
    pub fn with_floating(mut self, floating: bool) -> Self {
        self.floating = floating;
        self
    }
    /// Fluent setter for [`Self::monitor_id`].
    pub fn with_monitor_id(mut self, monitor_id: i32) -> Self {
        self.monitor_id = monitor_id;
        self
    }
    /// Fluent setter for [`Self::fullscreen`].
    pub fn with_fullscreen(mut self, fullscreen: bool) -> Self {
        self.fullscreen = fullscreen;
        self
    }
}

/// Compositor-neutral monitor/output.
///
/// Marked `#[non_exhaustive]` so additive field changes don't break
/// downstream match / struct-literal sites. External consumers construct
/// via [`WmMonitor::default`] plus the `with_*` setters below; same-crate
/// usage is unaffected by the attribute.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct WmMonitor {
    /// Compositor-assigned numeric ID. Matches [`WmClient::monitor_id`].
    pub id: i32,
    /// Output connector name (e.g. `DP-1`, `eDP-1`).
    pub name: String,
    /// Physical pixel width.
    pub width: i32,
    /// Physical pixel height.
    pub height: i32,
    /// Global x-offset in the compositor's layout.
    pub x: i32,
    /// Global y-offset in the compositor's layout.
    pub y: i32,
    /// HiDPI scale factor (1.0 = no scaling).
    pub scale: f64,
    /// Whether this monitor currently holds keyboard focus.
    pub focused: bool,
    /// Workspace that's active on this monitor.
    pub active_workspace: WmWorkspace,
}

impl WmMonitor {
    /// Fluent setter for [`Self::id`].
    pub fn with_id(mut self, id: i32) -> Self {
        self.id = id;
        self
    }
    /// Fluent setter for [`Self::name`].
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
    /// Fluent setter for [`Self::width`].
    pub fn with_width(mut self, width: i32) -> Self {
        self.width = width;
        self
    }
    /// Fluent setter for [`Self::height`].
    pub fn with_height(mut self, height: i32) -> Self {
        self.height = height;
        self
    }
    /// Fluent setter for [`Self::x`].
    pub fn with_x(mut self, x: i32) -> Self {
        self.x = x;
        self
    }
    /// Fluent setter for [`Self::y`].
    pub fn with_y(mut self, y: i32) -> Self {
        self.y = y;
        self
    }
    /// Fluent setter for [`Self::scale`].
    pub fn with_scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }
    /// Fluent setter for [`Self::focused`].
    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
    /// Fluent setter for [`Self::active_workspace`].
    pub fn with_active_workspace(mut self, active_workspace: WmWorkspace) -> Self {
        self.active_workspace = active_workspace;
        self
    }
}

/// Compositor-neutral workspace reference.
///
/// Marked `#[non_exhaustive]` so additive field changes don't break
/// downstream match / struct-literal sites. External consumers construct
/// via [`WmWorkspace::default`] plus the `with_*` setters below; same-crate
/// usage is unaffected by the attribute.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct WmWorkspace {
    /// Compositor-assigned numeric workspace ID.
    pub id: i32,
    /// Workspace name (may be numeric-as-string or a human name like `chat`).
    pub name: String,
}

impl WmWorkspace {
    /// Fluent setter for [`Self::id`].
    pub fn with_id(mut self, id: i32) -> Self {
        self.id = id;
        self
    }
    /// Fluent setter for [`Self::name`].
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

/// Events from the compositor event stream.
///
/// Marked `#[non_exhaustive]` so additional variants (e.g. the pending
/// [`WorkspaceChanged`](#) variant tracked in #127) can be added without
/// breaking downstream pattern-match sites. Consumers should include a
/// `_ => ...` fallback arm.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WmEvent {
    /// Active window changed. Contains the window id.
    ActiveWindowChanged(String),
    /// Monitor added or removed (hotplug).
    MonitorChanged,
    /// Any other event.
    Other(String),
}
