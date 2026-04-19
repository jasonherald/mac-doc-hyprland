/// Converts Go-style single-dash flags to clap-compatible double-dash flags.
///
/// Go's `flag` package uses single-dash for all flags (e.g. `-hd 20`, `-ico path`).
/// Clap only supports single-dash for single-character flags.
/// This preprocessor converts known Go-style flags so existing user configs
/// continue to work after the Go→Rust migration.
pub fn normalize_legacy_flags(
    args: impl Iterator<Item = String>,
    legacy_flags: &'static [&'static str],
) -> Vec<String> {
    let mut result = Vec::new();
    let mut passthrough = false;
    for arg in args {
        if passthrough {
            result.push(arg);
            continue;
        }
        if arg == "--" {
            passthrough = true;
            result.push(arg);
            continue;
        }
        match rewrite_arg(&arg, legacy_flags) {
            Some(rewritten) => result.push(rewritten),
            None => result.push(arg),
        }
    }
    result
}

/// Rewrites a single argument if it's a recognized Go-style legacy flag,
/// returning `None` to leave it unchanged otherwise. Splitting this out
/// keeps the main loop shallow enough to stay under the cognitive-
/// complexity budget (sonar rust:S3776).
fn rewrite_arg(arg: &str, legacy_flags: &'static [&'static str]) -> Option<String> {
    // Map -v to --version (Go compatibility for nwg-shell config utility)
    if arg == "-v" {
        return Some("--version".to_string());
    }
    let name = arg.strip_prefix('-')?;
    if name.starts_with('-') {
        return None;
    }
    if let Some((flag, value)) = name.split_once('=') {
        legacy_flags
            .contains(&flag)
            .then(|| format!("--{}={}", flag, value))
    } else {
        legacy_flags.contains(&name).then(|| format!("--{}", name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FLAGS: &[&str] = &["hd", "ico", "opacity", "wm"];

    #[test]
    fn converts_single_dash_flag() {
        let args = vec!["test".into(), "-hd".into(), "50".into()];
        let result = normalize_legacy_flags(args.into_iter(), TEST_FLAGS);
        assert_eq!(result, vec!["test", "--hd", "50"]);
    }

    #[test]
    fn converts_flag_with_equals() {
        let args = vec!["test".into(), "-hd=50".into(), "-ico=launcher".into()];
        let result = normalize_legacy_flags(args.into_iter(), TEST_FLAGS);
        assert_eq!(result, vec!["test", "--hd=50", "--ico=launcher"]);
    }

    #[test]
    fn preserves_double_dash() {
        let args = vec!["test".into(), "--hd".into(), "50".into()];
        let result = normalize_legacy_flags(args.into_iter(), TEST_FLAGS);
        assert_eq!(result, vec!["test", "--hd", "50"]);
    }

    #[test]
    fn preserves_unknown_single_dash() {
        let args = vec!["test".into(), "-x".into()];
        let result = normalize_legacy_flags(args.into_iter(), TEST_FLAGS);
        assert_eq!(result, vec!["test", "-x"]);
    }

    #[test]
    fn preserves_single_char_flags() {
        let args = vec!["test".into(), "-d".into()];
        let result = normalize_legacy_flags(args.into_iter(), TEST_FLAGS);
        assert_eq!(result, vec!["test", "-d"]);
    }

    #[test]
    fn converts_v_to_version() {
        let args = vec!["test".into(), "-v".into()];
        let result = normalize_legacy_flags(args.into_iter(), TEST_FLAGS);
        assert_eq!(result, vec!["test", "--version"]);
    }

    #[test]
    fn stops_normalizing_after_double_dash() {
        let args = vec!["test".into(), "--".into(), "-v".into(), "-hd".into()];
        let result = normalize_legacy_flags(args.into_iter(), TEST_FLAGS);
        assert_eq!(result, vec!["test", "--", "-v", "-hd"]);
    }
}
