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
/// Returns the first matching category name, or "Other".
pub fn assign_category(categories_field: &str) -> &'static str {
    let known = [
        "AudioVideo", "Development", "Game", "Graphics", "Network", "Office", "System", "Utility",
    ];

    for cat in categories_field.split(';') {
        let cat = cat.trim();
        if known.contains(&cat) {
            return known.iter().find(|&&k| k == cat).unwrap();
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
}
