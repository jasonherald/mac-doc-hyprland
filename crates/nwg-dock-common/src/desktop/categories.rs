/// A freedesktop application category.
#[derive(Debug, Clone)]
pub struct Category {
    pub name: String,
    pub display_name: String,
    pub icon: String,
}

/// The standard freedesktop main categories.
/// (name, display_name, icon) tuples for each standard category.
const CATEGORY_DEFS: &[(&str, &str, &str)] = &[
    ("AudioVideo", "Audio & Video", "applications-multimedia"),
    ("Development", "Development", "applications-development"),
    ("Game", "Games", "applications-games"),
    ("Graphics", "Graphics", "applications-graphics"),
    ("Network", "Internet", "applications-internet"),
    ("Office", "Office", "applications-office"),
    ("System", "System", "applications-system"),
    ("Utility", "Utilities", "applications-utilities"),
    ("Other", "Other", "applications-other"),
];

pub fn default_categories() -> Vec<Category> {
    CATEGORY_DEFS
        .iter()
        .map(|(name, display, icon)| Category {
            name: (*name).into(),
            display_name: (*display).into(),
            icon: (*icon).into(),
        })
        .collect()
}

/// Assigns an entry to ALL matching main categories based on its Categories field.
///
/// Returns a vec of category names. An app with `Categories=Development;Network;`
/// will appear in both Development and Network lists (matching Go behavior).
///
/// Handles secondary categories: Science/Education→Office, Settings/PackageManager→System,
/// Audio/Video→AudioVideo, etc.
pub fn assign_categories(categories_field: &str) -> Vec<&'static str> {
    let primary = [
        "AudioVideo",
        "Development",
        "Game",
        "Graphics",
        "Network",
        "Office",
        "System",
        "Utility",
    ];

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

    let mut result = Vec::new();

    for cat in categories_field.split(';') {
        let cat = cat.trim();
        if cat.is_empty() {
            continue;
        }
        if let Some(&matched) = primary.iter().find(|&&k| k == cat) {
            if !result.contains(&matched) {
                result.push(matched);
            }
        } else if let Some(&(_, mapped)) = secondary.iter().find(|&&(k, _)| k == cat)
            && !result.contains(&mapped)
        {
            result.push(mapped);
        }
    }

    if result.is_empty() {
        result.push("Other");
    }

    result
}

/// Convenience: returns the first matching category (for simple use cases).
pub fn assign_category(categories_field: &str) -> &'static str {
    assign_categories(categories_field)
        .into_iter()
        .next()
        .unwrap_or("Other")
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

    #[test]
    fn multi_category_assignment() {
        let cats = assign_categories("Development;Network;");
        assert!(cats.contains(&"Development"));
        assert!(cats.contains(&"Network"));
        assert_eq!(cats.len(), 2);
    }

    #[test]
    fn multi_category_dedup() {
        // Audio and AudioVideo both map to AudioVideo — should appear once
        let cats = assign_categories("Audio;AudioVideo;");
        assert_eq!(cats, vec!["AudioVideo"]);
    }
}
