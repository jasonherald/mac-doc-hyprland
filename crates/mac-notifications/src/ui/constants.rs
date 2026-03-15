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

/// Vertical padding around popup content (used for stacking height estimate).
pub const POPUP_PADDING: i32 = 24;

/// Maximum lines of body text shown in popup.
pub const POPUP_MAX_BODY_LINES: i32 = 3;

/// Max chars for popup summary line.
pub const POPUP_SUMMARY_CHARS: i32 = 40;

/// Max chars for popup body text.
pub const POPUP_BODY_CHARS: i32 = 50;

/// Width of the notification history panel.
pub const PANEL_WIDTH: i32 = 380;

/// Panel slide animation duration in ms.
pub const PANEL_REVEAL_DURATION_MS: u32 = 200;

/// Icon size in panel notification rows.
pub const PANEL_ICON_SIZE: i32 = 36;

/// Icon size in panel group headers.
pub const GROUP_ICON_SIZE: i32 = 24;

/// Max chars for panel row summary.
pub const PANEL_SUMMARY_CHARS: i32 = 35;

/// Max chars for panel row body.
pub const PANEL_BODY_CHARS: i32 = 45;

/// Max lines for panel row body.
pub const PANEL_BODY_LINES: i32 = 2;
