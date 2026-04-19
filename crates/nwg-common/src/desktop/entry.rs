use std::io::BufRead;
use std::path::Path;

/// A parsed XDG .desktop file entry.
#[derive(Debug, Clone, Default)]
pub struct DesktopEntry {
    pub desktop_id: String,
    pub name: String,
    pub name_loc: String,
    pub comment: String,
    pub comment_loc: String,
    pub icon: String,
    pub exec: String,
    pub category: String,
    pub terminal: bool,
    pub no_display: bool,
    /// StartupWMClass from the .desktop file — used to match compositor window class
    /// to desktop ID when they differ (e.g. "com.billz.app" → "billz").
    pub startup_wm_class: String,
}

/// Parses a .desktop file at the given path.
pub fn parse_desktop_file(id: &str, path: &Path) -> std::io::Result<DesktopEntry> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    Ok(parse_desktop_entry(id, reader))
}

/// Parses a .desktop entry from any reader.
pub fn parse_desktop_entry<R: BufRead>(id: &str, reader: R) -> DesktopEntry {
    let lang = std::env::var("LANG")
        .unwrap_or_default()
        .split('_')
        .next()
        .unwrap_or("")
        .to_string();

    let localized_name = format!("Name[{}]", lang);
    let localized_comment = format!("Comment[{}]", lang);
    let current_desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();

    let mut entry = DesktopEntry {
        desktop_id: id.to_string(),
        ..Default::default()
    };

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Stop at non-Desktop Entry sections
        if line.starts_with('[') && line != "[Desktop Entry]" {
            break;
        }

        let (key, value) = match parse_keypair(&line) {
            Some(kv) => kv,
            None => continue,
        };

        if value.is_empty() {
            continue;
        }

        apply_key_value(
            &mut entry,
            key,
            value,
            &localized_name,
            &localized_comment,
            &current_desktop,
        );
    }

    // Fallback: if localized name not found, use base name
    if entry.name_loc.is_empty() {
        entry.name_loc.clone_from(&entry.name);
    }
    if entry.comment_loc.is_empty() {
        entry.comment_loc.clone_from(&entry.comment);
    }

    entry
}

/// Applies a single key-value pair from a .desktop file to the entry.
fn apply_key_value(
    entry: &mut DesktopEntry,
    key: &str,
    value: &str,
    localized_name: &str,
    localized_comment: &str,
    current_desktop: &str,
) {
    match key {
        "Name" => entry.name = value.to_string(),
        "Comment" => entry.comment = value.to_string(),
        "Icon" => entry.icon = value.to_string(),
        "Categories" => entry.category = value.to_string(),
        "Terminal" => entry.terminal = value.parse().unwrap_or(false),
        "NoDisplay" if !entry.no_display => {
            entry.no_display = value.parse().unwrap_or(false);
        }
        "Hidden" if !entry.no_display => {
            entry.no_display = value.parse().unwrap_or(false);
        }
        "OnlyShowIn" if !entry.no_display => {
            entry.no_display = !desktop_list_contains(value, current_desktop);
        }
        "NotShowIn" if !entry.no_display && !current_desktop.is_empty() => {
            entry.no_display = desktop_list_contains(value, current_desktop);
        }
        "Exec" => {
            // Preserve quotes — proper shell splitting happens at launch time
            // via shell_words::split() (issue #11)
            entry.exec = value.to_string();
        }
        "StartupWMClass" => entry.startup_wm_class = value.to_string(),
        k if k == localized_name => entry.name_loc = value.to_string(),
        k if k == localized_comment => entry.comment_loc = value.to_string(),
        _ => {}
    }
}

/// Returns true if the semicolon-separated desktop list contains the given desktop name.
fn desktop_list_contains(list: &str, desktop: &str) -> bool {
    !desktop.is_empty()
        && list
            .split(';')
            .any(|item| !item.is_empty() && item == desktop)
}

/// Splits a line at the first `=` into (key, value), both trimmed.
fn parse_keypair(s: &str) -> Option<(&str, &str)> {
    let idx = s.find('=')?;
    if idx == 0 {
        return None;
    }
    Some((s[..idx].trim(), s[idx + 1..].trim()))
}

/// Strips desktop field codes (%u, %F, %%, etc.) from an Exec command.
/// Per the freedesktop Desktop Entry spec, recognised single-letter codes are
/// removed and `%%` is collapsed to a literal `%`. Arguments after field codes
/// are preserved. Quotes are preserved — shell splitting happens at launch time
/// via shell_words::split() (issue #11).
pub fn strip_field_codes(exec: &str) -> String {
    let mut result = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.peek() {
                // %% → literal %
                Some('%') => {
                    chars.next();
                    result.push('%');
                }
                // Known field codes per freedesktop spec — drop them
                Some(
                    'f' | 'F' | 'u' | 'U' | 'd' | 'D' | 'n' | 'N' | 'i' | 'c' | 'k' | 'v' | 'm',
                ) => {
                    chars.next();
                    // Trim a single leading space before the field code if present
                    if result.ends_with(' ') && chars.peek().is_none_or(|&ch| ch == ' ') {
                        result.pop();
                    }
                }
                // Unknown %-sequence — keep as-is
                _ => result.push('%'),
            }
        } else {
            result.push(c);
        }
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parse_basic_entry() {
        let desktop = "[Desktop Entry]\n\
            Name=Firefox\n\
            Comment=Web Browser\n\
            Icon=firefox\n\
            Exec=firefox %u\n\
            Categories=Network;WebBrowser;\n\
            Terminal=false\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("firefox", reader);
        assert_eq!(entry.name, "Firefox");
        assert_eq!(entry.icon, "firefox");
        assert_eq!(entry.exec, "firefox %u");
        assert!(!entry.terminal);
        assert!(!entry.no_display);
    }

    #[test]
    fn stops_at_non_desktop_entry_section() {
        let desktop = "[Desktop Entry]\n\
            Name=App\n\
            Icon=app\n\
            [Desktop Action New]\n\
            Name=Should Not Parse\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("app", reader);
        assert_eq!(entry.name, "App");
    }

    #[test]
    fn hidden_sets_no_display() {
        let desktop = "[Desktop Entry]\n\
            Name=Hidden\n\
            Hidden=true\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("hidden", reader);
        assert!(entry.no_display);
    }

    #[test]
    fn whitespace_handling() {
        let desktop = "[Desktop Entry]\n\
            Name  =  Spaced App  \n\
            Icon = spaced-icon \n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("spaced", reader);
        assert_eq!(entry.name, "Spaced App");
        assert_eq!(entry.icon, "spaced-icon");
    }

    #[test]
    fn missing_name_returns_empty() {
        let desktop = "[Desktop Entry]\n\
            Icon=myicon\n\
            Exec=myapp --start\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("myapp", reader);
        assert_eq!(entry.name, "");
        assert_eq!(entry.icon, "myicon");
        assert_eq!(entry.exec, "myapp --start");
    }

    #[test]
    fn nodisplay_true() {
        let desktop = "[Desktop Entry]\n\
            Name=Hidden App\n\
            NoDisplay=true\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("hidden-app", reader);
        assert!(entry.no_display);
    }

    #[test]
    fn only_show_in_matching() {
        // OnlyShowIn lists desktops that will never match the test
        // environment (which is either Hyprland or unset). In both
        // cases "Unity;MATE;" won't contain a match, so no_display
        // should be true.
        let desktop = "[Desktop Entry]\n\
            Name=Unity Only\n\
            OnlyShowIn=Unity;MATE;\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("unity-only", reader);
        assert!(entry.no_display);
    }

    #[test]
    fn not_show_in_matching() {
        // NotShowIn=GNOME; should NOT set no_display because the guard
        // requires current_desktop to be non-empty — and in the test
        // environment XDG_CURRENT_DESKTOP is typically unset.
        let desktop = "[Desktop Entry]\n\
            Name=Not GNOME\n\
            NotShowIn=GNOME;\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("not-gnome", reader);
        assert!(!entry.no_display);
    }

    #[test]
    fn exec_quotes_preserved() {
        let desktop = "[Desktop Entry]\n\
            Name=Browser\n\
            Exec=\"firefox\" %u\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("firefox", reader);
        assert_eq!(entry.exec, "\"firefox\" %u");
    }

    #[test]
    fn exec_complex_quotes_preserved() {
        let desktop = "[Desktop Entry]\n\
            Name=ShellCmd\n\
            Exec=sh -c \"printf 'Hello World'\"\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("shellcmd", reader);
        assert_eq!(entry.exec, "sh -c \"printf 'Hello World'\"");
    }

    #[test]
    fn malformed_lines_skipped() {
        let desktop = "[Desktop Entry]\n\
            NoEquals\n\
            =ValueOnly\n\
            \n\
            Name=Test\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("test", reader);
        assert_eq!(entry.name, "Test");
        // The other malformed lines should not cause any fields to be set
        assert_eq!(entry.icon, "");
        assert_eq!(entry.exec, "");
    }

    #[test]
    fn terminal_flag_parsing() {
        let desktop_true = "[Desktop Entry]\n\
            Name=Term App\n\
            Terminal=true\n";
        let reader = Cursor::new(desktop_true);
        let entry = parse_desktop_entry("term", reader);
        assert!(entry.terminal);

        let desktop_false = "[Desktop Entry]\n\
            Name=GUI App\n\
            Terminal=false\n";
        let reader = Cursor::new(desktop_false);
        let entry = parse_desktop_entry("gui", reader);
        assert!(!entry.terminal);
    }

    #[test]
    fn locale_name_fallback() {
        let desktop = "[Desktop Entry]\n\
            Name=English\n\
            Name[zz]=Zzz\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("loc", reader);
        // LANG is unlikely to start with "zz", so name_loc should
        // fall back to the base Name value.
        assert_eq!(entry.name_loc, "English");
    }

    #[test]
    fn startup_wm_class_parsed() {
        let desktop = "[Desktop Entry]\n\
            Name=Slack\n\
            Icon=slack\n\
            StartupWMClass=Slack\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("slack", reader);
        assert_eq!(entry.startup_wm_class, "Slack");
    }

    #[test]
    fn missing_startup_wm_class_is_empty() {
        let desktop = "[Desktop Entry]\n\
            Name=Simple\n\
            Icon=simple\n";
        let reader = Cursor::new(desktop);
        let entry = parse_desktop_entry("simple", reader);
        assert!(entry.startup_wm_class.is_empty());
    }

    #[test]
    fn strip_field_codes_basic() {
        assert_eq!(strip_field_codes("firefox %u"), "firefox");
        assert_eq!(strip_field_codes("code %F"), "code");
        assert_eq!(strip_field_codes("gimp"), "gimp");
    }

    #[test]
    fn strip_field_codes_preserves_quotes() {
        assert_eq!(strip_field_codes(r#""firefox" %u"#), r#""firefox""#);
        assert_eq!(
            strip_field_codes(r#"sh -c "echo hello" %u"#),
            r#"sh -c "echo hello""#
        );
    }

    #[test]
    fn strip_field_codes_no_space_before_percent() {
        assert_eq!(strip_field_codes("firefox%u"), "firefox");
    }

    #[test]
    fn strip_field_codes_preserves_args_after_code() {
        assert_eq!(strip_field_codes("foo %U --new-window"), "foo --new-window");
        assert_eq!(
            strip_field_codes("bar %f --flag %F --other"),
            "bar --flag --other"
        );
    }

    #[test]
    fn strip_field_codes_literal_percent() {
        assert_eq!(
            strip_field_codes(r#"sh -c "printf '100%%'""#),
            r#"sh -c "printf '100%'""#
        );
    }

    #[test]
    fn strip_field_codes_preserves_inner_whitespace() {
        assert_eq!(
            strip_field_codes(r#"sh -c "printf 'a  b'" %u"#),
            r#"sh -c "printf 'a  b'""#
        );
    }

    #[test]
    fn strip_field_codes_trims_whitespace() {
        assert_eq!(strip_field_codes("  firefox  "), "firefox");
        assert_eq!(strip_field_codes(""), "");
    }
}
