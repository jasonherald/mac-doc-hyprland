//! UI layout constants for the notification daemon.

/// Width of popup notification windows.
pub const POPUP_WIDTH: i32 = 380;

/// Margin from the top edge of screen.
pub const POPUP_TOP_MARGIN: i32 = 12;

/// Margin from the right/left edge of screen.
pub const POPUP_SIDE_MARGIN: i32 = 16;

/// Vertical gap between stacked popups.
pub const POPUP_GAP: i32 = 10;

/// Icon size in popup notifications.
pub const POPUP_ICON_SIZE: i32 = 48;

/// Default popup timeout in ms (macOS uses ~7 seconds).
pub const DEFAULT_POPUP_TIMEOUT_MS: u64 = 7000;

/// Border radius for popup windows.
pub const POPUP_BORDER_RADIUS: i32 = 12;

/// Maximum lines of body text shown in popup.
pub const POPUP_MAX_BODY_LINES: i32 = 3;

/// Width of the notification history panel.
pub const PANEL_WIDTH: i32 = 380;

/// Icon size in panel notification rows.
pub const PANEL_ICON_SIZE: i32 = 36;

/// Icon size in panel group headers.
pub const GROUP_ICON_SIZE: i32 = 24;
