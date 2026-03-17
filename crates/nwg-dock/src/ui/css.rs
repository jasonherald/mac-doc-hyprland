use nwg_dock_common::config::css;
use std::path::Path;

/// GTK4 overrides for mac-style dock rendering.
/// Compact buttons, transparent background, tight indicator spacing.
const GTK4_COMPAT_CSS: &str = r#"
window {
    background-color: rgba(54, 54, 79, 0.75);
}
.dock-button {
    min-height: 0;
    min-width: 0;
}
.dock-button image {
    margin: 0;
    padding: 0;
}
.dock-indicator {
    margin: 0;
    padding: 0;
    min-height: 0;
    min-width: 0;
}

/* Drag-to-reorder */
.dock-item {
    transition: margin 150ms ease-in-out;
}

/* Remove ALL GTK4 default drag-over highlighting (green outlines) */
*:drop(active) {
    outline: none;
    border-color: transparent;
    box-shadow: none;
}

/* Source item while being dragged */
.dragging-source {
    opacity: 0.2;
}

/* Source item when cursor is outside dock (will be removed on drop) */
.drag-will-remove {
    opacity: 0.3;
    background-color: rgba(255, 60, 60, 0.15);
    border-radius: 8px;
}
"#;

/// Loads the dock's CSS file and applies GTK4 compatibility overrides.
/// Returns true if the user's CSS was loaded successfully.
pub fn load_dock_css(css_path: &Path, opacity: u8) -> bool {
    let result = css::load_css(css_path);
    // Apply GTK4 button overrides at higher priority so they take effect
    // after the user's style.css
    css::load_css_from_data(GTK4_COMPAT_CSS);

    // Apply user-configurable opacity (overrides both user CSS and compat CSS)
    let alpha = opacity.min(100) as f64 / 100.0;
    let opacity_css = format!(
        "window {{ background-color: rgba(54, 54, 79, {:.2}); }}",
        alpha
    );
    css::load_css_from_data(&opacity_css);

    result
}
