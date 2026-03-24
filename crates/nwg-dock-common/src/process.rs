use std::fs;

/// Reads the command line of a process by PID from `/proc/<pid>/cmdline`
/// and returns it as a properly shell-quoted string.
///
/// Uses `shell_words::join()` to handle arguments containing spaces,
/// quotes, and other special characters safely.
///
/// Called via `--dump-args <pid>` during `make upgrade` to capture
/// running process arguments before restarting.
pub fn dump_args(pid: u32) -> Option<String> {
    let path = format!("/proc/{}/cmdline", pid);
    let data = fs::read(&path).ok()?;
    let mut args: Vec<String> = data
        .split(|&b| b == 0)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    // /proc/cmdline has a trailing null → remove the empty string it produces
    if matches!(args.last(), Some(s) if s.is_empty()) {
        args.pop();
    }
    if args.is_empty() {
        return None;
    }
    Some(shell_words::join(&args))
}

/// Checks if `--dump-args <pid>` was passed and handles it.
/// Returns `true` if handled (caller should exit), `false` otherwise.
pub fn handle_dump_args() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--dump-args") {
        if let Some(pid_str) = args.get(pos + 1)
            && let Ok(pid) = pid_str.parse::<u32>()
        {
            match dump_args(pid) {
                Some(cmdline) => {
                    println!("{}", cmdline);
                    std::process::exit(0);
                }
                None => {
                    eprintln!("Failed to read cmdline for pid {}", pid);
                    std::process::exit(1);
                }
            }
        }
        eprintln!("Usage: --dump-args <pid>");
        std::process::exit(1);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_args_current_process() {
        let pid = std::process::id();
        let result = dump_args(pid);
        assert!(result.is_some());
        // Should contain our test binary name at minimum
        let cmdline = result.unwrap();
        assert!(!cmdline.is_empty());
    }

    #[test]
    fn dump_args_nonexistent_pid() {
        assert!(dump_args(999_999_999).is_none());
    }

    #[test]
    fn shell_quoting_preserves_spaces() {
        // Verify shell_words::join handles args with spaces
        let args = vec!["cmd".to_string(), "arg with spaces".to_string()];
        let joined = shell_words::join(&args);
        assert_eq!(joined, "cmd 'arg with spaces'");
    }

    #[test]
    fn shell_quoting_preserves_nested_quotes() {
        let args = vec![
            "nwg-drawer".to_string(),
            "-c".to_string(),
            r#"nwg-dialog -p exit -c "loginctl terminate-user \"\"""#.to_string(),
        ];
        let joined = shell_words::join(&args);
        // Should round-trip: split the joined string back and get the same args
        let split = shell_words::split(&joined).unwrap();
        assert_eq!(split, args);
    }
}
