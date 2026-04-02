/// Edge detection threshold in pixels from the screen edge (autohide trigger zone).
pub const EDGE_THRESHOLD: i32 = 2;

/// Thickness of the Sway hotspot trigger window in pixels.
pub const HOTSPOT_THICKNESS: i32 = 4;

/// Pixel margin beyond the dock bounds before a drag-off triggers unpin.
pub const DRAG_OUTSIDE_MARGIN: f64 = 30.0;

/// Minimum pointer movement (in pixels) before a GestureDrag claims the
/// event sequence and suppresses Button::clicked. Matches GTK's default
/// DnD drag threshold. Without this, even 1px of movement during a click
/// would suppress the app launch (issue #30).
pub const DRAG_CLAIM_THRESHOLD: f64 = 8.0;

/// Maximum time (in seconds) to show the launch bounce animation.
/// After this, the animation stops even if no matching window appeared.
pub const LAUNCH_ANIMATION_TIMEOUT_SECS: u64 = 10;
