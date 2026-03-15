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

/// Launches a command string via `hyprctl dispatch exec`.
///
/// This is the preferred method for Hyprland — it ensures proper
/// window tracking, workspace assignment, and process isolation.
pub fn launch_hyprctl(command: &str) {
    let command = command.replace('"', "");
    if command.trim().is_empty() {
        log::error!("Empty command to launch");
        return;
    }
    log::info!("Launching via hyprctl: {}", command);
    let _ = crate::hyprland::ipc::hyprctl(&format!("dispatch exec {}", command));
}

/// Launches a command string via `hyprctl dispatch exec` with terminal wrapping.
pub fn launch_hyprctl_terminal(command: &str, term: &str) {
    let full = format!("{} -e {}", term, command);
    launch_hyprctl(&full);
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
