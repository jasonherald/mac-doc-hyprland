use crate::compositor::Compositor;
use crate::desktop::icons::get_exec;
use std::path::PathBuf;
use std::process::Command;

/// Launches an application by its class/app ID.
///
/// Resolves the Exec command from .desktop files and runs it.
/// Uses direct spawn (for dock, which manages its own process lifecycle).
pub fn launch(app_id: &str, app_dirs: &[PathBuf]) {
    let command = get_exec(app_id, app_dirs).unwrap_or_else(|| app_id.to_string());
    launch_command(&command);
}

/// Launches a command via the compositor's exec mechanism,
/// or via uwsm if the `wm` flag was set to "uwsm".
pub fn launch_via_compositor(command: &str, compositor: &dyn Compositor) {
    let command = command.replace('"', "");
    if command.trim().is_empty() {
        log::error!("Empty command to launch");
        return;
    }

    // Check if uwsm launch mode is active (set via set_uwsm_mode)
    if USE_UWSM.load(std::sync::atomic::Ordering::Relaxed) {
        launch_via_uwsm(&command);
        return;
    }

    log::info!("Launching via compositor: {}", command);
    if let Err(e) = compositor.exec(&command) {
        log::error!("Failed to launch: {}", e);
    }
}

/// Global flag for uwsm launch mode (set once at startup from --wm flag).
static USE_UWSM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Enables uwsm launch mode. Called at startup when `--wm uwsm` is detected.
pub fn set_uwsm_mode(enabled: bool) {
    USE_UWSM.store(enabled, std::sync::atomic::Ordering::Relaxed);
    if enabled {
        log::info!("Launch mode: uwsm app --");
    }
}

/// Launches a command via `uwsm app --` for proper session management.
fn launch_via_uwsm(command: &str) {
    let command = command.trim();
    if command.is_empty() {
        return;
    }
    log::info!("Launching via uwsm: {}", command);
    let parts: Vec<&str> = command.split_whitespace().collect();
    match Command::new("uwsm")
        .arg("app")
        .arg("--")
        .args(&parts)
        .spawn()
    {
        Ok(_) => {}
        Err(e) => {
            log::warn!("uwsm not found, falling back to direct launch: {}", e);
            launch_command(command);
        }
    }
}

/// Launches a command with terminal wrapping via the compositor.
pub fn launch_terminal_via_compositor(command: &str, term: &str, compositor: &dyn Compositor) {
    let full = format!("{} -e {}", term, command);
    launch_via_compositor(&full, compositor);
}

/// Launches a raw command string directly (without WM dispatch).
/// Used by the dock for launcher commands and direct process spawning.
pub fn launch_command(command: &str) {
    let command = command.replace('"', "");

    let elements: Vec<&str> = command.split_whitespace().collect();
    if elements.is_empty() {
        log::error!("Empty command to launch");
        return;
    }

    // Find prepended env variables (KEY=VALUE)
    let mut env_vars = Vec::new();
    let mut cmd_idx = 0;

    for (idx, item) in elements.iter().enumerate() {
        if item.contains('=') {
            env_vars.push(*item);
        } else if !item.starts_with('-') && cmd_idx == 0 {
            cmd_idx = idx;
            break;
        }
    }

    log::info!("Launching: '{}'", elements[cmd_idx..].join(" "));

    let mut cmd = Command::new(elements[cmd_idx]);
    cmd.args(&elements[cmd_idx + 1..]);

    if !env_vars.is_empty() {
        for var in &env_vars {
            if let Some((key, value)) = var.split_once('=') {
                cmd.env(key, value);
            }
        }
    }

    match cmd.spawn() {
        Ok(_) => {}
        Err(e) => log::error!("Unable to launch command: {}", e),
    }
}
