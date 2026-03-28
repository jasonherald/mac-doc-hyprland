use crate::compositor::Compositor;
use crate::desktop::icons::get_exec;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::mpsc;

/// Interval between try_wait polls in the child-reaper thread.
const REAPER_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// Sends a child process to the shared reaper thread for exit status monitoring.
/// Uses try_wait polling so long-running GUI apps don't block reaping of others.
pub fn reap_child(child: Child, label: String) {
    static SENDER: std::sync::OnceLock<std::sync::Mutex<mpsc::Sender<(Child, String)>>> =
        std::sync::OnceLock::new();

    let sender = SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<(Child, String)>();
        if let Err(e) = std::thread::Builder::new()
            .name("child-reaper".into())
            .spawn(move || reaper_loop(rx))
        {
            log::error!("Failed to spawn child-reaper thread: {}", e);
            // Fallback is safe: send() will fail and callers synchronously reap.
        }
        std::sync::Mutex::new(tx)
    });

    enqueue_or_reap_sync(sender, child, label);
}

/// Main loop for the child-reaper thread: receives children and polls them.
fn reaper_loop(rx: mpsc::Receiver<(Child, String)>) {
    let mut children: Vec<(Child, String)> = Vec::new();
    loop {
        match rx.recv_timeout(REAPER_POLL_INTERVAL) {
            Ok(job) => children.push(job),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) if children.is_empty() => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }
        poll_children(&mut children);
    }
}

/// Non-blocking poll of all tracked children, removing finished ones.
fn poll_children(children: &mut Vec<(Child, String)>) {
    let mut i = 0;
    while i < children.len() {
        match children[i].0.try_wait() {
            Ok(Some(status)) => {
                let (_, cmd) = children.swap_remove(i);
                if !status.success() {
                    log::warn!("Shell command '{}' exited with {}", cmd, status);
                }
            }
            Ok(None) => i += 1, // Still running
            Err(e) => {
                let (_, cmd) = children.swap_remove(i);
                log::warn!("Failed to wait on shell command '{}': {}", cmd, e);
            }
        }
    }
}

/// Enqueues a child for reaping, or waits synchronously if the reaper is unavailable.
fn enqueue_or_reap_sync(
    sender: &std::sync::Mutex<mpsc::Sender<(Child, String)>>,
    mut child: Child,
    label: String,
) {
    // Send under the lock, then drop the guard before any blocking wait
    let send_err = match sender.lock() {
        Ok(tx) => {
            let result = tx.send((child, label.clone()));
            drop(tx);
            result.err()
        }
        Err(e) => {
            log::error!("Reaper mutex poisoned for '{}': {}", label, e);
            if let Err(wait_err) = child.wait() {
                log::warn!("Failed to wait on child '{}': {}", label, wait_err);
            }
            return;
        }
    };
    if let Some(e) = send_err {
        log::error!("Reaper channel closed for '{}': {}", label, e);
        let (mut orphan, _) = e.0;
        if let Err(wait_err) = orphan.wait() {
            log::warn!("Failed to wait on orphaned child '{}': {}", label, wait_err);
        }
    }
}

/// Spawns a command and hands the child to the reaper thread.
/// For fire-and-forget processes where we don't need the output.
pub fn spawn_and_forget(mut cmd: Command, label: &str) {
    match cmd.spawn() {
        Ok(child) => reap_child(child, label.to_string()),
        Err(e) => log::error!("Failed to spawn '{}': {}", label, e),
    }
}

/// Launches an application by its class/app ID.
///
/// Resolves the Exec command from .desktop files and runs it.
/// Uses direct spawn (for dock, which manages its own process lifecycle).
pub fn launch(app_id: &str, app_dirs: &[PathBuf]) {
    let command = get_exec(app_id, app_dirs).unwrap_or_else(|| app_id.to_string());
    launch_command(&command);
}

/// Prepends `GTK_THEME=` to a command if force-theme is enabled.
pub fn prepend_theme(cmd: &str, theme_prefix: &str) -> String {
    if theme_prefix.is_empty() {
        cmd.to_string()
    } else {
        format!("{} {}", theme_prefix, cmd)
    }
}

/// Launches a .desktop entry's Exec command via the compositor.
///
/// Handles the full pipeline: field code stripping, theme prepend,
/// and terminal detection. Shared by all drawer launch sites.
/// Quotes are preserved end-to-end (PR #11).
pub fn launch_desktop_entry(
    exec: &str,
    terminal: bool,
    term_cmd: &str,
    theme_prefix: &str,
    compositor: &dyn Compositor,
) {
    let clean = crate::desktop::entry::strip_field_codes(exec);
    if clean.is_empty() {
        log::debug!("Skipping launch: exec string is empty after stripping field codes");
        return;
    }
    let cmd = prepend_theme(&clean, theme_prefix);
    if terminal {
        launch_terminal_via_compositor(&cmd, term_cmd, compositor);
    } else {
        launch_via_compositor(&cmd, compositor);
    }
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
        Ok(child) => reap_child(child, command.to_string()),
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
    match Command::new("sh").args(["-c", command]).spawn() {
        Ok(child) => reap_child(child, command.to_string()),
        Err(e) => log::error!("Failed to spawn shell command '{}': {}", command, e),
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

    spawn_and_forget(cmd, &cmd_args.join(" "));
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
                && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
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
    fn prepend_theme_empty_prefix() {
        assert_eq!(prepend_theme("firefox", ""), "firefox");
    }

    #[test]
    fn prepend_theme_with_prefix() {
        assert_eq!(
            prepend_theme("firefox", "GTK_THEME=Adwaita:dark"),
            "GTK_THEME=Adwaita:dark firefox"
        );
    }

    #[test]
    fn launch_desktop_entry_empty_exec_is_noop() {
        // Exec that reduces to empty after field code stripping should not panic
        // (can't test compositor launch without a live compositor, but we can
        // verify the empty-exec early return path)
        use crate::desktop::entry::strip_field_codes;
        assert!(strip_field_codes("%u").is_empty());
        assert!(strip_field_codes("%F").is_empty());
    }

    #[test]
    fn shell_command_empty_is_noop() {
        // Should not panic or spawn anything
        launch_shell_command("");
        launch_shell_command("   ");
    }

    const WAIT_RETRIES: usize = 40; // 2s total with 50ms interval
    const WAIT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

    /// Polls a file until a readiness predicate matches, or times out.
    fn wait_for_file(path: &std::path::Path, ready: impl Fn(&str) -> bool) -> String {
        for _ in 0..WAIT_RETRIES {
            std::thread::sleep(WAIT_INTERVAL);
            if let Ok(c) = std::fs::read_to_string(path)
                && ready(&c)
            {
                return c;
            }
        }
        panic!("Timed out waiting for {}", path.display());
    }

    /// Removes a file, ignoring NotFound but panicking on other errors.
    fn remove_test_file(path: &std::path::Path) {
        if let Err(e) = std::fs::remove_file(path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            panic!("Failed to remove {}: {}", path.display(), e);
        }
    }

    #[test]
    fn shell_command_handles_nested_quotes() {
        // Simulate nwg-piotr's power bar command with nested quotes.
        let tmp = std::env::temp_dir().join("nwg-shell-test-output");
        remove_test_file(&tmp);

        let cmd = format!(r#"sh -c "echo 'hello world' > '{}'""#, tmp.display());
        launch_shell_command(&cmd);

        let content = wait_for_file(&tmp, |c| c.trim() == "hello world");
        assert_eq!(content.trim(), "hello world");
        remove_test_file(&tmp);
    }

    #[test]
    fn shell_command_handles_complex_quoting() {
        // Simulates: nwg-dialog -p exit -c "loginctl terminate-user \"\""
        let tmp = std::env::temp_dir().join("nwg-shell-test-complex");
        remove_test_file(&tmp);

        let cmd = format!(
            r#"printf '%s\n' "arg with spaces" "another 'nested' arg" > '{}'"#,
            tmp.display()
        );
        launch_shell_command(&cmd);

        // Wait for both lines to be written
        let content = wait_for_file(&tmp, |c| c.trim().lines().count() >= 2);
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines, vec!["arg with spaces", "another 'nested' arg"]);
        remove_test_file(&tmp);
    }
}
