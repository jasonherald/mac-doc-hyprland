use clap::Parser;

/// A macOS-style dock for Hyprland.
#[derive(Parser, Debug, Clone)]
#[command(name = "mac-dock", version, about)]
pub struct DockConfig {
    /// Alignment in full width/height: "start", "center" or "end"
    #[arg(short = 'a', long, default_value = "center")]
    pub alignment: String,

    /// Auto-hide: show dock when hotspot hovered, close when left or button clicked
    #[arg(short = 'd', long)]
    pub autohide: bool,

    /// CSS file name
    #[arg(short = 's', long, default_value = "style.css")]
    pub css_file: String,

    /// Turn on debug messages
    #[arg(long)]
    pub debug: bool,

    /// Set exclusive zone: move other windows aside
    #[arg(short = 'x', long)]
    pub exclusive: bool,

    /// Take full screen width/height
    #[arg(short = 'f', long)]
    pub full: bool,

    /// Quote-delimited, space-separated class list to ignore in the dock
    #[arg(short = 'g', long, default_value = "")]
    pub ignore_classes: String,

    /// Hotspot delay in ms (smaller = faster trigger)
    #[arg(long, default_value_t = 20)]
    pub hotspot_delay: i64,

    /// Hotspot layer: "overlay" or "top"
    #[arg(long, default_value = "overlay")]
    pub hotspot_layer: String,

    /// Alternative name or path for the launcher icon
    #[arg(long, default_value = "")]
    pub ico: String,

    /// Ignore running apps on these workspaces (comma-separated names/ids)
    #[arg(long, default_value = "")]
    pub ignore_workspaces: String,

    /// Icon size in pixels
    #[arg(short = 'i', long, default_value_t = 48)]
    pub icon_size: i32,

    /// Command assigned to the launcher button
    #[arg(short = 'c', long, default_value = "mac-drawer")]
    pub launcher_cmd: String,

    /// Launcher button position: "start" or "end"
    #[arg(long, default_value = "end")]
    pub launcher_pos: String,

    /// Layer: "overlay", "top" or "bottom"
    #[arg(short = 'l', long, default_value = "overlay")]
    pub layer: String,

    /// Margin bottom
    #[arg(long, default_value_t = 0)]
    pub mb: i32,

    /// Margin left
    #[arg(long, default_value_t = 0)]
    pub ml: i32,

    /// Margin right
    #[arg(long, default_value_t = 0)]
    pub mr: i32,

    /// Margin top
    #[arg(long, default_value_t = 0)]
    pub mt: i32,

    /// Don't show the launcher button
    #[arg(long)]
    pub nolauncher: bool,

    /// Number of workspaces you use
    #[arg(short = 'w', long, default_value_t = 10)]
    pub num_ws: i32,

    /// Position: "bottom", "top", "left" or "right"
    #[arg(short = 'p', long, default_value = "bottom")]
    pub position: String,

    /// Leave the program resident, but without hotspot
    #[arg(short = 'r', long)]
    pub resident: bool,

    /// Name of output to display the dock on
    #[arg(short = 'o', long, default_value = "")]
    pub output: String,

    /// Allow multiple instances of the dock
    #[arg(short = 'm', long)]
    pub multi: bool,
}

impl DockConfig {
    /// Whether the dock orientation is vertical (left/right position).
    pub fn is_vertical(&self) -> bool {
        self.position == "left" || self.position == "right"
    }

    /// Whether this is a resident-mode dock (autohide or resident flag).
    pub fn is_resident_mode(&self) -> bool {
        self.autohide || self.resident
    }

    /// Returns ignored workspace names/ids as a list.
    pub fn ignored_workspaces(&self) -> Vec<String> {
        if self.ignore_workspaces.is_empty() {
            Vec::new()
        } else {
            self.ignore_workspaces.split(',').map(|s| s.trim().to_string()).collect()
        }
    }

    /// Returns ignored classes as a list.
    pub fn ignored_classes(&self) -> Vec<String> {
        if self.ignore_classes.is_empty() {
            Vec::new()
        } else {
            self.ignore_classes.split(' ').map(|s| s.trim().to_string()).collect()
        }
    }
}
