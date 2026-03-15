use crate::desktop::icons::get_exec;
use std::path::PathBuf;
use std::process::Command;

/// Launches an application by its class/app ID.
///
/// Resolves the Exec command from .desktop files and runs it via `hyprctl dispatch exec`.
pub fn launch(app_id: &str, app_dirs: &[PathBuf]) {
    let command = get_exec(app_id, app_dirs).unwrap_or_else(|| app_id.to_string());

    // Remove any remaining quotation marks
    let command = command.replace('"', "");

    let elements: Vec<&str> = command.split_whitespace().collect();
    if elements.is_empty() {
        log::error!("Empty command for {}", app_id);
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

    let args: Vec<&str> = elements[cmd_idx + 1..]
        .iter()
        .filter(|a| !a.contains('='))
        .copied()
        .collect();

    log::info!(
        "env vars: {:?}; command: '{}'; args: {:?}",
        env_vars,
        elements[cmd_idx],
        args
    );

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
