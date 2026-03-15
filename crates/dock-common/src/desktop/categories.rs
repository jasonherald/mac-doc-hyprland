/// A freedesktop application category.
#[derive(Debug, Clone)]
pub struct Category {
    pub name: String,
    pub display_name: String,
    pub icon: String,
}

/// The standard freedesktop main categories.
pub fn default_categories() -> Vec<Category> {
    vec![
        Category {
            name: "AudioVideo".into(),
            display_name: "Audio & Video".into(),
            icon: "applications-multimedia".into(),
        },
        Category {
            name: "Development".into(),
            display_name: "Development".into(),
            icon: "applications-development".into(),
        },
        Category {
            name: "Game".into(),
            display_name: "Games".into(),
            icon: "applications-games".into(),
        },
        Category {
            name: "Graphics".into(),
            display_name: "Graphics".into(),
            icon: "applications-graphics".into(),
        },
        Category {
            name: "Network".into(),
            display_name: "Internet".into(),
            icon: "applications-internet".into(),
        },
        Category {
            name: "Office".into(),
            display_name: "Office".into(),
            icon: "applications-office".into(),
        },
        Category {
            name: "System".into(),
            display_name: "System".into(),
            icon: "applications-system".into(),
        },
        Category {
            name: "Utility".into(),
            display_name: "Utilities".into(),
            icon: "applications-utilities".into(),
        },
        Category {
            name: "Other".into(),
            display_name: "Other".into(),
            icon: "applications-other".into(),
        },
    ]
}

/// Assigns an entry to a main category based on its Categories field.
/// Returns the first matching main category name, or "Other".
///
/// Handles secondary categories: Science/Education→Office, Settings/PackageManager→System,
/// Audio/Video→AudioVideo, etc.
pub fn assign_category(categories_field: &str) -> &'static str {
    let primary = [
        "AudioVideo", "Development", "Game", "Graphics", "Network", "Office", "System", "Utility",
    ];

    // Secondary → primary mappings
    let secondary: &[(&str, &str)] = &[
        ("Audio", "AudioVideo"),
        ("Video", "AudioVideo"),
        ("Science", "Office"),
        ("Education", "Office"),
        ("Settings", "System"),
        ("DesktopSettings", "System"),
        ("PackageManager", "System"),
        ("HardwareSettings", "System"),
    ];

    for cat in categories_field.split(';') {
        let cat = cat.trim();
        if cat.is_empty() {
            continue;
        }
        // Check primary match first
        if let Some(&matched) = primary.iter().find(|&&k| k == cat) {
            return matched;
        }
        // Check secondary mapping
        if let Some(&(_, mapped)) = secondary.iter().find(|&&(k, _)| k == cat) {
            return mapped;
        }
    }

    "Other"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigns_known_category() {
        assert_eq!(assign_category("Network;WebBrowser;"), "Network");
        assert_eq!(assign_category("Development;IDE;"), "Development");
    }

    #[test]
    fn assigns_other_for_unknown() {
        assert_eq!(assign_category("FooBar;Baz;"), "Other");
        assert_eq!(assign_category(""), "Other");
    }

    #[test]
    fn assigns_secondary_categories() {
        assert_eq!(assign_category("Science;Math;"), "Office");
        assert_eq!(assign_category("Education;"), "Office");
        assert_eq!(assign_category("Settings;DesktopSettings;"), "System");
        assert_eq!(assign_category("Audio;Player;"), "AudioVideo");
        assert_eq!(assign_category("PackageManager;"), "System");
    }
}
