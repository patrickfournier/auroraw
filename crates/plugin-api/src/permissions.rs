// SPDX-License-Identifier: MIT OR Apache-2.0
//! What a plugin declares it needs, shown to the person before installation (architecture §8.2,
//! decision D-055). Nothing here grants anything: the host decides what it actually hands a
//! running instance ([`crate::Family`], §8.3), and a plugin gets nothing beyond what it declared
//! and what the host granted.

use serde::{Deserialize, Serialize};

/// What a plugin asks for. Every field defaults to nothing, matching the host's own default of
/// no access unless granted (architecture §8.2). A decoder plugin (WP6's `rawler` plugin, the
/// spike's `rawimport`) needs none of this: the host hands it bytes and reads pixels back, never
/// touching the file system on the plugin's behalf.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permissions {
    /// Folders the plugin asks to read, as labels shown to the person (e.g. "the source being
    /// imported"), not host paths: the host decides the actual path when it grants the request.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_folders: Vec<String>,
    /// Network destinations the plugin asks to reach, as labels (e.g. a service's name); the
    /// host mediates every connection (architecture §8.2, §9.4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network: Vec<String>,
    /// Named secrets (API keys, tokens) the plugin asks for, kept in the system keychain
    /// (architecture §9.4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
    /// Whether the plugin asks to read the clock. Granted to every plugin in practice (spike 4:
    /// timing is not a meaningful side channel for this use), kept as an explicit field so the
    /// declaration stays honest about what a plugin can observe.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clock: bool,
}
