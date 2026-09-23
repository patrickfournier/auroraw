// SPDX-License-Identifier: MIT OR Apache-2.0
//! The plugin declaration (architecture §8.3, decision D-078): identifier, version, API version,
//! family, permissions. The host refuses a declaration it cannot satisfy and says why.
//!
//! D-078 also lists a panel, a pipeline stage with ordering constraints, and parameters with
//! limits and defaults: those describe an **operation** plugin's placement in the develop
//! pipeline, which does not exist before M2 (architecture §8.4, D-084). [`Declaration`] carries
//! them as optional fields so the schema does not have to change shape when operation plugins
//! arrive; nothing in this crate or WP6 gives them meaning yet.

use crate::Permissions;
use serde::{Deserialize, Serialize};

/// What family of capability a plugin provides (architecture §8.1). Only the two families M1
/// uses are named here; `Operation`, `Export` and others join when their work package builds
/// them, matching how `Family` itself only exists because the declaration schema does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Family {
    /// Lists, describes, reads and watches files (`plugin_api::Source`, WP4).
    Source,
    /// Decodes a file's own bytes to pixels (`plugin_api::Decoder`, WP6).
    Import,
}

/// A plugin's placement in the develop pipeline: which named stage it runs at, and constraints on
/// its order relative to others (architecture §8.3). Unused before an `Operation` family exists;
/// see this module's doc comment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    /// The pipeline stage this plugin runs at (e.g. `"scene-linear"`, spike 4's `gpuop`).
    pub stage: String,
    /// Must run after these other stages, if present in the pipeline.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    /// Must run before these other stages, if present in the pipeline.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
}

/// What every plugin declares (architecture §8.3, D-078), shown to the person before
/// installation and read by the host before it is trusted with anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declaration {
    /// A stable identifier for this plugin, unique in the index (architecture §8.7).
    pub identifier: String,
    /// The plugin's own version.
    pub version: String,
    /// The plugin API version this declaration was written against. The host refuses a
    /// declaration whose `api_version` it does not support.
    pub api_version: u32,
    /// What capability family this plugin provides.
    pub family: Family,
    /// The panel this plugin's settings appear under, if it has any (operation plugins only; see
    /// this module's doc comment).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
    /// This plugin's place in the develop pipeline, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    /// What the plugin asks the host to grant it.
    #[serde(default)]
    pub permissions: Permissions,
}

/// Why the host refused a declaration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeclarationError {
    /// The identifier is empty.
    #[error("a plugin's identifier cannot be empty")]
    EmptyIdentifier,
    /// The version is empty.
    #[error("a plugin's version cannot be empty")]
    EmptyVersion,
    /// The declaration was written against an API version this host does not support.
    #[error("this plugin needs API version {requested}, this host supports up to {supported}")]
    UnsupportedApiVersion {
        /// The version the declaration asked for.
        requested: u32,
        /// The highest version this host understands.
        supported: u32,
    },
    /// A stage placement names itself in its own ordering constraints.
    #[error("stage {stage:?} cannot be constrained relative to itself")]
    SelfConstrainedPlacement {
        /// The stage that names itself.
        stage: String,
    },
}

/// The plugin API version this host understands (architecture §8.2b): the interface is
/// experimental until M5, so this changes without a deprecation period until then.
pub const HOST_API_VERSION: u32 = 0;

impl Declaration {
    /// Checks that this declaration is well formed and that this host can satisfy it
    /// (architecture §8.3: "the host refuses a declaration it cannot satisfy and says why").
    /// Does not check permissions against what the person has actually granted; that happens at
    /// installation and at load time, not here.
    pub fn validate(&self) -> Result<(), DeclarationError> {
        if self.identifier.trim().is_empty() {
            return Err(DeclarationError::EmptyIdentifier);
        }
        if self.version.trim().is_empty() {
            return Err(DeclarationError::EmptyVersion);
        }
        if self.api_version > HOST_API_VERSION {
            return Err(DeclarationError::UnsupportedApiVersion {
                requested: self.api_version,
                supported: HOST_API_VERSION,
            });
        }
        if let Some(placement) = &self.placement
            && (placement.after.contains(&placement.stage)
                || placement.before.contains(&placement.stage))
        {
            return Err(DeclarationError::SelfConstrainedPlacement {
                stage: placement.stage.clone(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> Declaration {
        Declaration {
            identifier: "org.auroraw.rawler".into(),
            version: "0.1.0".into(),
            api_version: HOST_API_VERSION,
            family: Family::Import,
            panel: None,
            placement: None,
            permissions: Permissions::default(),
        }
    }

    #[test]
    fn a_well_formed_declaration_validates() {
        assert!(minimal().validate().is_ok());
    }

    #[test]
    fn an_empty_identifier_is_refused() {
        let mut d = minimal();
        d.identifier = "  ".into();
        assert_eq!(d.validate(), Err(DeclarationError::EmptyIdentifier));
    }

    #[test]
    fn an_empty_version_is_refused() {
        let mut d = minimal();
        d.version.clear();
        assert_eq!(d.validate(), Err(DeclarationError::EmptyVersion));
    }

    #[test]
    fn a_future_api_version_is_refused_with_both_numbers() {
        let mut d = minimal();
        d.api_version = HOST_API_VERSION + 1;
        assert_eq!(
            d.validate(),
            Err(DeclarationError::UnsupportedApiVersion {
                requested: HOST_API_VERSION + 1,
                supported: HOST_API_VERSION,
            })
        );
    }

    #[test]
    fn a_stage_constrained_relative_to_itself_is_refused() {
        let mut d = minimal();
        d.placement = Some(Placement {
            stage: "tone".into(),
            after: vec!["tone".into()],
            before: vec![],
        });
        assert_eq!(
            d.validate(),
            Err(DeclarationError::SelfConstrainedPlacement {
                stage: "tone".into()
            })
        );
    }

    #[test]
    fn round_trips_through_json() {
        let d = minimal();
        let text = serde_json::to_string(&d).unwrap();
        let back: Declaration = serde_json::from_str(&text).unwrap();
        assert_eq!(d, back);
    }
}
