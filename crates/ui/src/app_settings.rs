// SPDX-License-Identifier: GPL-3.0-or-later
//! What the application remembers about a person, not about a workspace: for now, the language of
//! the interface. A plain JSON file in the machine's data folder; a missing or unreadable file is a
//! first run, never an error.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The languages the interface is translated into (`i18n/<code>/`).
pub const LANGUAGES: &[&str] = &["en", "fr"];

/// The application's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// `system` (follow the machine) or one of [`LANGUAGES`].
    pub language: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "system".into(),
        }
    }
}

impl AppSettings {
    /// Reads `path`, or the defaults if it is missing or not readable as settings.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes `path`, best effort: failing to remember a language is not worth an error dialog.
    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let bytes = serde_json::to_vec_pretty(self).expect("settings always serialise");
        let _ = std::fs::write(path, bytes);
    }
}

/// The bundled language for a setting: itself when it is one of ours, else the machine's own when
/// that is one of ours, else English.
pub fn resolve_language(setting: &str) -> &'static str {
    let wanted = if LANGUAGES.contains(&setting) {
        setting.to_string()
    } else {
        sys_locale::get_locale().unwrap_or_default()
    };
    let primary = wanted
        .split(['-', '_', '.', '@'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    LANGUAGES
        .iter()
        .copied()
        .find(|language| *language == primary)
        .unwrap_or("en")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_corrupt_file_is_the_defaults_and_a_chosen_language_round_trips() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("nested/app-settings.json");
        assert_eq!(AppSettings::load(&path).language, "system");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(AppSettings::load(&path), AppSettings::default());

        let chosen = AppSettings {
            language: "fr".into(),
        };
        chosen.save(&path);
        assert_eq!(AppSettings::load(&path), chosen);
    }

    #[test]
    fn a_chosen_language_is_used_and_anything_else_falls_back_to_a_bundled_one() {
        assert_eq!(resolve_language("fr"), "fr");
        assert_eq!(resolve_language("en"), "en");
        // "system" and unknown codes resolve to something we translate, whatever the machine says.
        assert!(LANGUAGES.contains(&resolve_language("system")));
        assert!(LANGUAGES.contains(&resolve_language("klingon")));
    }
}
