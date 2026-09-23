// SPDX-License-Identifier: GPL-3.0-or-later
//! What the interface remembers between launches, per machine and per workspace: the fields of the
//! import view. A plain JSON file at a path the caller gives (this crate resolves no directory
//! itself, like every crate under `engine`); a missing or unreadable file is a first run, never an
//! error, since nothing here is worth refusing to start over.

use std::path::Path;

use auroraw_engine::{MetadataTemplate, PairRule, Profile};
use serde::{Deserialize, Serialize};

/// The folders and names an import needs, as a person typed them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The folder photos are copied into.
    pub archive: String,
    /// A second folder every photo is also copied into, or empty for none.
    pub backup: String,
    /// The folders-and-names template; empty means [`DEFAULT_TEMPLATE`].
    pub template: String,
    /// Written to every imported photo's sidecar.
    pub creator: String,
    /// Written to every imported photo's sidecar.
    pub rights: String,
    /// The last card or folder imported from.
    pub source: String,
}

/// Year folder, then a day folder, then the camera's own file name (M1 plan §6 item 6).
pub const DEFAULT_TEMPLATE: &str = "{year}/{date}/{original}.{ext}";

impl Settings {
    /// Reads `path`, or the defaults if it is missing or not readable as settings.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes `path`, best effort: failing to remember a field is not worth interrupting an
    /// import for.
    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let bytes = serde_json::to_vec_pretty(self).expect("settings always serialise");
        let _ = std::fs::write(path, bytes);
    }

    /// The template to import with.
    pub fn template_or_default(&self) -> &str {
        let template = self.template.trim();
        if template.is_empty() {
            DEFAULT_TEMPLATE
        } else {
            template
        }
    }

    /// The import profile these fields describe.
    pub fn profile(&self) -> Profile {
        let creator = self.creator.trim();
        let rights = self.rights.trim();
        Profile {
            name: "Default".into(),
            destination_template: self.template_or_default().to_string(),
            backup_templates: Vec::new(),
            pair_rule: PairRule::Both,
            metadata_template: MetadataTemplate {
                creator: if creator.is_empty() {
                    Vec::new()
                } else {
                    vec![creator.to_string()]
                },
                rights: (!rights.is_empty()).then(|| rights.to_string()),
                keyword_paths: Vec::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_corrupt_file_is_a_first_run() {
        let dir = auroraw_testkit::temp_dir();
        assert_eq!(
            Settings::load(&dir.path().join("none.json")),
            Settings::default()
        );
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, b"{ not json").unwrap();
        assert_eq!(Settings::load(&bad), Settings::default());
    }

    #[test]
    fn settings_round_trip_and_tolerate_fields_added_later() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("nested/settings.json");
        let settings = Settings {
            archive: "/photos".into(),
            creator: "Patrick".into(),
            ..Settings::default()
        };
        settings.save(&path);
        assert_eq!(Settings::load(&path), settings);
        std::fs::write(&path, br#"{"archive": "/a", "somethingNew": 1}"#).unwrap();
        assert_eq!(Settings::load(&path).archive, "/a");
    }

    #[test]
    fn the_profile_carries_the_typed_metadata_and_falls_back_to_the_default_template() {
        let mut settings = Settings::default();
        assert_eq!(settings.profile().destination_template, DEFAULT_TEMPLATE);
        assert!(settings.profile().metadata_template.creator.is_empty());
        settings.creator = " Patrick ".into();
        settings.rights = "© Patrick".into();
        settings.template = "{year}/{original}.{ext}".into();
        let profile = settings.profile();
        assert_eq!(profile.destination_template, "{year}/{original}.{ext}");
        assert_eq!(profile.metadata_template.creator, vec!["Patrick"]);
        assert_eq!(
            profile.metadata_template.rights.as_deref(),
            Some("© Patrick")
        );
    }
}
