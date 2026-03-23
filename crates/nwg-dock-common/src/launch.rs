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
    // Quotes are preserved — the compositor handles shell parsing internally
    if command.trim().is_empty() {
        log::error!("Empty command to launch");
        return;
    }

    // Check if uwsm launch mode is active (set via set_uwsm_mode)
    if USE_UWSM.load(std::sync::atomic::Ordering::Relaxed) {
        launch_via_uwsm(command);
        return;
    }

    log::info!("Launching via compositor: {}", command);
    if let Err(e) = compositor.exec(command) {
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
/// Uses shell_words::split for POSIX-compliant quoted argument handling.
/// Leading KEY=VALUE env assignments are extracted and applied via .env(),
/// matching the behavior of launch_command().
fn launch_via_uwsm(command: &str) {
    let command = command.trim();
    if command.is_empty() {
        return;
    }
    log::info!("Launching via uwsm: {}", command);
    let elements = split_command(command);
    let (env_vars, cmd_args) = extract_env_prefix(&elements);

    if cmd_args.is_empty() {
        log::error!("No command found after env vars in: {}", command);
        return;
    }

    let mut cmd = Command::new("uwsm");
    cmd.arg("app").arg("--").args(cmd_args);
    for (key, value) in &env_vars {
        cmd.env(key, value);
    }

    match cmd.spawn() {
        Ok(_) => {}
        Err(e) => {
            log::warn!("uwsm not found, falling back to direct launch: {}", e);
            launch_command(command);
        }
    }
}

/// Launches a user-provided shell command string via `sh -c`.
///
/// Handles complex quoting, pipes, redirects, and nested quotes correctly.
/// Use this for user-configured commands (power bar, launcher button, etc.)
/// where the command string is a full shell expression.
pub fn launch_shell_command(command: &str) {
    let command = command.trim();
    if command.is_empty() {
        return;
    }
    log::info!("Running shell command: {}", command);
    if let Err(e) = Command::new("sh").args(["-c", command]).spawn() {
        log::error!("Failed to run shell command '{}': {}", command, e);
    }
}

/// Launches a command with terminal wrapping via the compositor.
pub fn launch_terminal_via_compositor(command: &str, term: &str, compositor: &dyn Compositor) {
    let full = format!("{} -e {}", term, command);
    launch_via_compositor(&full, compositor);
}

/// Launches a raw command string directly (without WM dispatch).
/// Uses shell_words::split for POSIX-compliant quoted argument handling.
pub fn launch_command(command: &str) {
    let elements = split_command(command);
    if elements.is_empty() {
        log::error!("Empty command to launch");
        return;
    }

    let (env_vars, cmd_args) = extract_env_prefix(&elements);

    if cmd_args.is_empty() {
        log::error!("No command found after env vars in: {}", command);
        return;
    }

    log::info!("Launching: '{}'", cmd_args.join(" "));

    let mut cmd = Command::new(&cmd_args[0]);
    cmd.args(&cmd_args[1..]);
    for (key, value) in &env_vars {
        cmd.env(key, value);
    }

    match cmd.spawn() {
        Ok(_) => {}
        Err(e) => log::error!("Unable to launch command: {}", e),
    }
}

/// Extracts leading KEY=VALUE env assignments from a split command.
/// Returns (env_vars, remaining_args).
fn extract_env_prefix(elements: &[String]) -> (Vec<(&str, &str)>, &[String]) {
    let mut cmd_idx = 0;
    let mut env_vars = Vec::new();

    for (idx, item) in elements.iter().enumerate() {
        if let Some((key, value)) = item.split_once('=') {
            // Only treat as env var if key is a valid POSIX identifier
            // (starts with letter or underscore, rest alphanumeric or underscore)
            if !key.is_empty()
                && key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                env_vars.push((key, value));
                continue;
            }
        }
        cmd_idx = idx;
        break;
    }

    (env_vars, &elements[cmd_idx..])
}

/// Splits a command string into arguments using POSIX shell quoting rules.
/// Falls back to split_whitespace if the command has unbalanced quotes.
fn split_command(command: &str) -> Vec<String> {
    match shell_words::split(command) {
        Ok(parts) => parts,
        Err(e) => {
            log::warn!(
                "Unbalanced quotes in command '{}': {}, falling back to whitespace split",
                command,
                e
            );
            command.split_whitespace().map(String::from).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uwsm_empty_command_returns_early() {
        launch_via_uwsm("");
        launch_via_uwsm("   ");
    }

    #[test]
    fn uwsm_mode_toggle() {
        set_uwsm_mode(true);
        assert!(USE_UWSM.load(std::sync::atomic::Ordering::Relaxed));
        set_uwsm_mode(false);
        assert!(!USE_UWSM.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn split_command_quoted_args() {
        let parts = split_command(r#"sh -c "printf 'Hello World'""#);
        assert_eq!(parts, vec!["sh", "-c", "printf 'Hello World'"]);
    }

    #[test]
    fn split_command_simple() {
        let parts = split_command("firefox --new-window");
        assert_eq!(parts, vec!["firefox", "--new-window"]);
    }

    #[test]
    fn split_command_env_prefix() {
        let parts = split_command("GTK_THEME=Adwaita:dark firefox");
        assert_eq!(parts, vec!["GTK_THEME=Adwaita:dark", "firefox"]);
    }

    #[test]
    fn split_command_unbalanced_falls_back() {
        // Unbalanced quotes — should fall back to split_whitespace
        let parts = split_command("sh -c \"unterminated");
        assert!(!parts.is_empty()); // doesn't panic, returns something
    }

    #[test]
    fn split_command_empty() {
        let parts = split_command("");
        assert!(parts.is_empty());
    }

    #[test]
    fn extract_env_prefix_splits_correctly() {
        let elements: Vec<String> = vec!["GTK_THEME=Adwaita:dark", "firefox", "--new-window"]
            .into_iter()
            .map(String::from)
            .collect();
        let (env, cmd) = extract_env_prefix(&elements);
        assert_eq!(env, vec![("GTK_THEME", "Adwaita:dark")]);
        assert_eq!(cmd, &["firefox", "--new-window"]);
    }

    #[test]
    fn extract_env_prefix_no_env() {
        let elements: Vec<String> = vec!["firefox", "--new-window"]
            .into_iter()
            .map(String::from)
            .collect();
        let (env, cmd) = extract_env_prefix(&elements);
        assert!(env.is_empty());
        assert_eq!(cmd, &["firefox", "--new-window"]);
    }

    #[test]
    fn extract_env_prefix_rejects_digit_start() {
        let elements: Vec<String> = vec!["1VAR=bad", "firefox"]
            .into_iter()
            .map(String::from)
            .collect();
        let (env, cmd) = extract_env_prefix(&elements);
        assert!(env.is_empty());
        assert_eq!(cmd, &["1VAR=bad", "firefox"]);
    }

    #[test]
    fn shell_command_empty_is_noop() {
        // Should not panic or spawn anything
        launch_shell_command("");
        launch_shell_command("   ");
    }

    /// Polls a file until it exists and has content, or times out.
    fn wait_for_file(path: &std::path::Path) -> String {
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Ok(c) = std::fs::read_to_string(path) {
                if !c.is_empty() {
                    return c;
                }
            }
        }
        panic!("Timed out waiting for {}", path.display());
    }

    #[test]
    fn shell_command_handles_nested_quotes() {
        // Simulate nwg-piotr's power bar command with nested quotes.
        let tmp = std::env::temp_dir().join("nwg-shell-test-output");
        let _ = std::fs::remove_file(&tmp);

        let cmd = format!(
            r#"sh -c "echo 'hello world' > {}""#,
            tmp.display()
        );
        launch_shell_command(&cmd);

        let content = wait_for_file(&tmp);
        assert_eq!(content.trim(), "hello world");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn shell_command_handles_complex_quoting() {
        // Simulates: nwg-dialog -p exit -c "loginctl terminate-user \"\""
        let tmp = std::env::temp_dir().join("nwg-shell-test-complex");
        let _ = std::fs::remove_file(&tmp);

        let cmd = format!(
            r#"printf '%s\n' "arg with spaces" "another 'nested' arg" > {}"#,
            tmp.display()
        );
        launch_shell_command(&cmd);

        let content = wait_for_file(&tmp);
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines, vec!["arg with spaces", "another 'nested' arg"]);
        let _ = std::fs::remove_file(&tmp);
    }
}
