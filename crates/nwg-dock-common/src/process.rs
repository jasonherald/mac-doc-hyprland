use std::ffi::OsStr;
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
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("Failed to read {}: {}", path, e);
            return None;
        }
    };
    let args = parse_cmdline_bytes(&data);
    if args.is_empty() {
        return None;
    }
    Some(shell_words::join(&args))
}

/// Parses raw `/proc/<pid>/cmdline` bytes into a list of arguments.
/// Splits on NUL bytes, strips the trailing empty element from the terminator.
fn parse_cmdline_bytes(data: &[u8]) -> Vec<String> {
    let mut args: Vec<String> = data
        .split(|&b| b == 0)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    // /proc/cmdline has a trailing null → remove the empty string it produces
    if matches!(args.last(), Some(s) if s.is_empty()) {
        args.pop();
    }
    args
}

/// Checks if `--dump-args <pid>` was passed and handles it.
/// If present, prints the shell-quoted command line and terminates the process.
/// Returns only when the flag is absent.
/// Uses `args_os()` to avoid panicking on non-Unicode argv.
pub fn handle_dump_args() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let flag = OsStr::new("--dump-args");
    let Some(pos) = args.iter().position(|a| a == flag) else {
        return;
    };
    if let Some(pid_os) = args.get(pos + 1)
        && let Some(pid_str) = pid_os.to_str()
        && let Ok(pid) = pid_str.parse::<u32>()
    {
        match dump_args(pid) {
            Some(cmdline) => {
                println!("{}", cmdline);
                std::process::exit(0); // Success: printed cmdline
            }
            None => {
                eprintln!("Failed to read cmdline for pid {}", pid);
                std::process::exit(1); // Error: couldn't read /proc
            }
        }
    }
    eprintln!("Usage: --dump-args <pid>");
    std::process::exit(1); // Error: missing or invalid PID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_args_current_process_round_trips() {
        // dump_args reads /proc/self/cmdline and shell-quotes it.
        // Splitting the result back should produce valid args.
        let pid = std::process::id();
        let result = dump_args(pid).expect("should read own cmdline");
        assert!(!result.is_empty());
        let round_tripped = shell_words::split(&result).expect("should parse back");
        assert!(!round_tripped.is_empty());
    }

    #[test]
    fn dump_args_nonexistent_pid() {
        assert!(dump_args(999_999_999).is_none()); // Intentionally beyond Linux pid_max
    }

    #[test]
    fn join_split_round_trip_with_spaces() {
        let args = vec!["cmd".to_string(), "arg with spaces".to_string()];
        let joined = shell_words::join(&args);
        let split = shell_words::split(&joined).unwrap();
        assert_eq!(split, args);
    }

    #[test]
    fn join_split_round_trip_nested_quotes() {
        // Simulates nwg-piotr's power bar command with nested quotes
        let args = vec![
            "nwg-drawer".to_string(),
            "-c".to_string(),
            r#"nwg-dialog -p exit -c "loginctl terminate-user \"\"""#.to_string(),
        ];
        let joined = shell_words::join(&args);
        let split = shell_words::split(&joined).unwrap();
        assert_eq!(split, args);
    }

    #[test]
    fn join_split_round_trip_empty_arg() {
        // An argument that is an empty string should survive the round trip
        let args = vec!["cmd".to_string(), "".to_string(), "last".to_string()];
        let joined = shell_words::join(&args);
        let split = shell_words::split(&joined).unwrap();
        assert_eq!(split, args);
    }

    #[test]
    fn parse_cmdline_bytes_basic() {
        let data = b"cmd\0--flag\0value\0";
        assert_eq!(parse_cmdline_bytes(data), vec!["cmd", "--flag", "value"]);
    }

    #[test]
    fn parse_cmdline_bytes_empty_middle_arg() {
        // Empty argument between two others (e.g. -c "")
        let data = b"cmd\0\0last\0";
        assert_eq!(parse_cmdline_bytes(data), vec!["cmd", "", "last"]);
    }

    #[test]
    fn parse_cmdline_bytes_no_trailing_null() {
        // Some edge cases may lack the trailing null
        let data = b"cmd\0arg";
        assert_eq!(parse_cmdline_bytes(data), vec!["cmd", "arg"]);
    }

    #[test]
    fn parse_cmdline_bytes_empty() {
        assert!(parse_cmdline_bytes(b"").is_empty());
    }
}
