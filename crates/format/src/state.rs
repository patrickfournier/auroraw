// SPDX-License-Identifier: GPL-3.0-or-later
//! The state files (design note 002): JSON with an envelope, written canonically.
//!
//! Every file has `"format"` and `"schema"` first, then its own fields in the order they are
//! declared here, then any key this version does not know, sorted, in `extra`. The same content
//! always gives the same bytes, so a file that did not change need not be rewritten. A file whose
//! schema is newer than the reader's is not interpreted (and so never modified).

use auroraw_types::{
    CollectionId, KeywordId, MemberRef, PhotoId, SchemaVersion, SeriesId, SourceId, Timestamp,
    WorkspaceId,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Why a state file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// The bytes are not valid JSON, or a field has the wrong shape.
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The top level is not an object with a `format` and a `schema`.
    #[error("not a state file: missing or malformed envelope")]
    Envelope,
    /// The file is a state file of another kind.
    #[error("expected the format {expected:?}, found {found:?}")]
    Format {
        /// The format expected.
        expected: &'static str,
        /// The format found.
        found: String,
    },
}

/// The result of reading a state file.
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded<T> {
    /// A schema this version understands.
    Current(T),
    /// A schema newer than this version knows: not interpreted, never to be rewritten.
    Newer(SchemaVersion),
}

impl<T> Loaded<T> {
    /// The content, if the schema is understood.
    pub fn current(self) -> Option<T> {
        match self {
            Self::Current(t) => Some(t),
            Self::Newer(_) => None,
        }
    }
}

/// The body of a state file: its format name and the schema this version writes.
pub trait StateBody: Serialize + DeserializeOwned {
    /// The value of the `"format"` key.
    const FORMAT: &'static str;
    /// The schema this version reads and writes.
    const SCHEMA: u32;
}

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    format: String,
    schema: SchemaVersion,
    #[serde(flatten)]
    body: T,
}

/// The canonical bytes of a state file: indented JSON, sorted unknown keys, a final newline.
pub fn write_state<T: StateBody>(body: &T) -> Vec<u8> {
    let envelope = Envelope {
        format: T::FORMAT.to_string(),
        schema: SchemaVersion(T::SCHEMA),
        body,
    };
    let mut text = serde_json::to_string_pretty(&envelope).expect("state files serialise");
    text.push('\n');
    text.into_bytes()
}

/// Reads a state file.
pub fn read_state<T: StateBody>(bytes: &[u8]) -> Result<Loaded<T>, StateError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let value: Value = serde_json::from_slice(bytes)?;
    let object = value.as_object().ok_or(StateError::Envelope)?;
    let format = object
        .get("format")
        .and_then(Value::as_str)
        .ok_or(StateError::Envelope)?;
    if format != T::FORMAT {
        return Err(StateError::Format {
            expected: T::FORMAT,
            found: format.to_string(),
        });
    }
    let schema = object
        .get("schema")
        .and_then(Value::as_u64)
        .ok_or(StateError::Envelope)?;
    if schema > u64::from(T::SCHEMA) {
        return Ok(Loaded::Newer(SchemaVersion(
            u32::try_from(schema).unwrap_or(u32::MAX),
        )));
    }
    let envelope: Envelope<T> = serde_json::from_value(value)?;
    Ok(Loaded::Current(envelope.body))
}

fn skip_none<T>(o: &Option<T>) -> bool {
    o.is_none()
}

/// Keys this version does not know, kept and written back (sorted).
pub type Extra = Map<String, Value>;

/// The marker of a workspace, `workspace.json` (design note 001 §5.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    /// The layout version of the folder (note 001 §5.8).
    pub layout: u32,
    /// The identifier of the workspace.
    pub workspace_id: WorkspaceId,
    /// The catalogue's name.
    pub catalogue_name: String,
    /// When the workspace was created.
    pub created: Timestamp,
    /// The application version that created it.
    pub created_by: String,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Extra,
}

impl StateBody for Marker {
    const FORMAT: &'static str = "auroraw/workspace";
    const SCHEMA: u32 = 1;
}

/// A keyword of the vocabulary (note 002 §6.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeywordEntry {
    /// Its identifier.
    pub id: KeywordId,
    /// Its name; never contains `|`.
    pub name: String,
    /// Its parent, or none at the top level.
    pub parent: Option<KeywordId>,
    /// Alternative names.
    #[serde(default)]
    pub synonyms: Vec<String>,
    /// Whether it is written to exports (the "do not export" flag, D-045, is `false`).
    #[serde(default = "yes")]
    pub export: bool,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Extra,
}

fn yes() -> bool {
    true
}

/// `vocabulary.json`: the keyword tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vocabulary {
    /// When it last changed.
    pub updated: Timestamp,
    /// The keywords, written sorted by identifier.
    pub keywords: Vec<KeywordEntry>,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Extra,
}

impl StateBody for Vocabulary {
    const FORMAT: &'static str = "auroraw/vocabulary";
    const SCHEMA: u32 = 1;
}

impl Vocabulary {
    /// The canonical bytes: keywords sorted by identifier, so a rename changes one entry.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = self.clone();
        v.keywords.sort_by_key(|k| k.id);
        write_state(&v)
    }
}

/// A source (note 002 §6.3). `config` is opaque and belongs to the source plugin; `hint` is only
/// a location from the machine that last wrote the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceEntry {
    /// Its identifier.
    pub id: SourceId,
    /// The source plugin's identifier.
    pub kind: String,
    /// Its display name.
    pub name: String,
    /// A location hint from the last machine; never authoritative.
    #[serde(default)]
    pub hint: Extra,
    /// File name patterns to ignore.
    #[serde(default)]
    pub ignore: Vec<String>,
    /// The plugin's own settings, opaque.
    #[serde(default)]
    pub config: Extra,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Extra,
}

/// `sources.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sources {
    /// When it last changed.
    pub updated: Timestamp,
    /// The sources, in the order the photographer arranged them.
    pub sources: Vec<SourceEntry>,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Extra,
}

impl StateBody for Sources {
    const FORMAT: &'static str = "auroraw/sources";
    const SCHEMA: u32 = 1;
}

/// A collection kind written by this version.
pub mod collection_kind {
    /// A manual collection.
    pub const MANUAL: &str = "manual";
    /// A smart collection defined by a query.
    pub const SMART: &str = "smart";
    /// A client selection filled from a publication (M4).
    pub const CLIENT_SELECTION: &str = "client-selection";
}

/// `collections/<id>.json` (note 002 §6.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    /// Its identifier.
    pub id: CollectionId,
    /// When it last changed.
    pub updated: Timestamp,
    /// Its name.
    pub name: String,
    /// `manual`, `smart` or `client-selection`; an unknown kind is kept as written.
    pub kind: String,
    /// The collection it is inside, or none.
    pub parent: Option<CollectionId>,
    /// The ordered members of a manual collection.
    #[serde(default)]
    pub members: Vec<MemberRef>,
    /// The query of a smart collection: opaque here, defined with the search.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub query: Option<Value>,
    /// The version of the query language.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub query_schema: Option<u32>,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Extra,
}

impl StateBody for Collection {
    const FORMAT: &'static str = "auroraw/collection";
    const SCHEMA: u32 = 1;
}

/// `series/<xx>/<id>.json` (note 002 §6.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Series {
    /// Its identifier.
    pub id: SeriesId,
    /// When it last changed.
    pub updated: Timestamp,
    /// `burst`, `bracket`, `similar` or `manual`; an unknown kind is kept as written.
    pub kind: String,
    /// The cover photo.
    pub cover: PhotoId,
    /// Whether the series was resolved.
    pub resolved: bool,
    /// The photos designated to keep when resolving.
    #[serde(default)]
    pub kept: Vec<PhotoId>,
    /// The members, in capture order.
    pub members: Vec<PhotoId>,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Extra,
}

impl StateBody for Series {
    const FORMAT: &'static str = "auroraw/series";
    const SCHEMA: u32 = 1;
}
