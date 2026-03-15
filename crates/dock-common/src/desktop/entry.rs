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

        match key {
            "Name" => entry.name = value.to_string(),
            "Comment" => entry.comment = value.to_string(),
            "Icon" => entry.icon = value.to_string(),
            "Categories" => entry.category = value.to_string(),
            "Terminal" => entry.terminal = value.parse().unwrap_or(false),
            "NoDisplay"
                if !entry.no_display => {
                    entry.no_display = value.parse().unwrap_or(false);
                }
            "Hidden"
                if !entry.no_display => {
                    entry.no_display = value.parse().unwrap_or(false);
                }
            "OnlyShowIn"
                if !entry.no_display => {
                    entry.no_display = true;
                    if !current_desktop.is_empty() {
                        for item in value.split(';') {
                            if !item.is_empty() && item == current_desktop {
                                entry.no_display = false;
                            }
                        }
                    }
                }
            "NotShowIn"
                if !entry.no_display && !current_desktop.is_empty() => {
                    for item in value.split(';') {
                        if !item.is_empty() && item == current_desktop {
                            entry.no_display = true;
                        }
                    }
                }
            "Exec" => {
                entry.exec = value.replace(['"', '\''], "");
            }
            k if k == localized_name => entry.name_loc = value.to_string(),
            k if k == localized_comment => entry.comment_loc = value.to_string(),
            _ => {}
        }
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

/// Splits a line at the first `=` into (key, value), both trimmed.
fn parse_keypair(s: &str) -> Option<(&str, &str)> {
    let idx = s.find('=')?;
    if idx == 0 {
        return None;
    }
    Some((s[..idx].trim(), s[idx + 1..].trim()))
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
}
