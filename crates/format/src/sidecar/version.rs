// SPDX-License-Identifier: GPL-3.0-or-later
use auroraw_types::{ContentHash, KeywordId, PhotoId, SchemaVersion, Timestamp, VersionId};

use super::SidecarError;
use super::extract as x;
use super::metadata::Metadata;
use super::photo::PhotoSidecar;
use crate::state::Loaded;
use crate::xmp::{ArrayKind, Property, Xmp, ns};

/// The schema of the Auroraw part of the version sidecar this version reads and writes.
pub const VERSION_SCHEMA: u32 = 1;

/// A field a version can override (spec §5.5). Keywords are additive and are not in this list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideField {
    /// The rating.
    Rating,
    /// The flag.
    Flag,
    /// The colour label.
    Label,
    /// The title.
    Title,
    /// The caption.
    Caption,
}

impl OverrideField {
    fn as_text(self) -> &'static str {
        match self {
            Self::Rating => "rating",
            Self::Flag => "flag",
            Self::Label => "label",
            Self::Title => "title",
            Self::Caption => "caption",
        }
    }
}

impl std::str::FromStr for OverrideField {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "rating" => Ok(Self::Rating),
            "flag" => Ok(Self::Flag),
            "label" => Ok(Self::Label),
            "title" => Ok(Self::Title),
            "caption" => Ok(Self::Caption),
            _ => Err(()),
        }
    }
}

/// The sidecar of a version: its own choices, and a derived copy of the effective metadata.
///
/// The photo sidecar plus this version's overrides and added keywords are the truth; `meta` is a
/// copy, refreshed to match and repaired if it disagrees (note 003 §5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct VersionSidecar {
    /// The version's identifier.
    pub version_id: VersionId,
    /// The photo it belongs to.
    pub photo_id: PhotoId,
    /// The version's name.
    pub name: Option<String>,
    /// When it was created.
    pub created: Option<Timestamp>,
    /// The fields it defines itself.
    pub overrides: Vec<OverrideField>,
    /// The keywords it adds to the photo's.
    pub added_keyword_ids: Vec<KeywordId>,
    /// The digest of the photo's metadata at the time of the copy.
    pub copied_from: Option<ContentHash>,
    /// The effective metadata: the version's value for an overridden field, the photo's otherwise.
    pub meta: Metadata,
    /// Properties this version does not know (the development, in M2), kept and written back.
    pub extra: Vec<Property>,
}

impl VersionSidecar {
    /// A new version of a photo, whose copy is taken from the photo.
    pub fn new(photo: &PhotoSidecar, version_id: VersionId) -> Self {
        let mut v = Self {
            version_id,
            photo_id: photo.photo_id,
            name: None,
            created: None,
            overrides: Vec::new(),
            added_keyword_ids: Vec::new(),
            copied_from: None,
            meta: Metadata::default(),
            extra: Vec::new(),
        };
        v.refresh_copy(photo);
        v
    }

    /// Whether the copy is up to date with the photo (note 003 §5.3): a comparison of digests.
    pub fn is_copy_current(&self, photo: &PhotoSidecar) -> bool {
        self.copied_from == Some(photo.meta.digest())
    }

    /// Rebuilds the copy from the photo, keeping this version's own values for the fields it
    /// overrides and its added keywords. The photo and the overrides win over the copy.
    pub fn refresh_copy(&mut self, photo: &PhotoSidecar) {
        let own = self.meta.clone();
        let mut m = photo.meta.clone();
        for field in &self.overrides {
            match field {
                OverrideField::Rating => m.rating = own.rating,
                OverrideField::Flag => m.flag = own.flag,
                OverrideField::Label => m.label = own.label.clone(),
                OverrideField::Title => m.title = own.title.clone(),
                OverrideField::Caption => m.caption = own.caption.clone(),
            }
        }
        // The union of the photo's keywords and the ones this version adds, by identifier.
        let own_pairs: Vec<(KeywordId, String)> = own
            .keyword_ids
            .iter()
            .copied()
            .zip(own.keyword_paths.iter().cloned())
            .filter(|(id, _)| self.added_keyword_ids.contains(id))
            .collect();
        for (id, path) in own_pairs {
            if !m.keyword_ids.contains(&id) {
                m.push_keyword(id, path);
            }
        }
        self.meta = m;
        self.copied_from = Some(photo.meta.digest());
    }

    /// The XMP model, in the canonical order.
    pub fn to_xmp(&self) -> Xmp {
        let mut props = vec![
            Property::text(ns::AUR, "Schema", VERSION_SCHEMA.to_string()),
            Property::text(ns::AUR, "VersionId", self.version_id.to_string()),
            Property::text(ns::AUR, "PhotoId", self.photo_id.to_string()),
        ];
        props.extend(x::opt_text(ns::AUR, "Name", &self.name));
        props.extend(
            self.created
                .map(|t| Property::text(ns::AUR, "Created", t.to_string())),
        );
        let overrides: Vec<String> = self
            .overrides
            .iter()
            .map(|f| f.as_text().to_string())
            .collect();
        props.extend(x::opt_array(
            ns::AUR,
            "Overrides",
            ArrayKind::Bag,
            &overrides,
        ));
        let added: Vec<String> = self
            .added_keyword_ids
            .iter()
            .map(ToString::to_string)
            .collect();
        props.extend(x::opt_array(
            ns::AUR,
            "AddedKeywordIds",
            ArrayKind::Bag,
            &added,
        ));
        props.extend(
            self.copied_from
                .map(|d| Property::text(ns::AUR, "CopiedFrom", d.to_string())),
        );
        props.extend(self.meta.to_properties());
        props.extend(self.extra.iter().cloned());
        Xmp {
            properties: props,
            prefixes: Vec::new(),
        }
    }

    /// The canonical bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_xmp().to_bytes()
    }

    /// Reads a version sidecar. A schema newer than this version knows is reported, not interpreted.
    pub fn from_bytes(bytes: &[u8]) -> Result<Loaded<Self>, SidecarError> {
        Self::from_xmp(Xmp::from_bytes(bytes)?)
    }

    /// Reads a version sidecar from its XMP model.
    pub fn from_xmp(xmp: Xmp) -> Result<Loaded<Self>, SidecarError> {
        let mut props = xmp.properties;
        let schema = x::parsed::<u32>(&mut props, ns::AUR, "Schema")
            .ok_or(SidecarError::Missing("aur:Schema"))?;
        if schema > VERSION_SCHEMA {
            return Ok(Loaded::Newer(SchemaVersion(schema)));
        }
        let version_id = x::parsed(&mut props, ns::AUR, "VersionId")
            .ok_or(SidecarError::Missing("aur:VersionId"))?;
        let photo_id = x::parsed(&mut props, ns::AUR, "PhotoId")
            .ok_or(SidecarError::Missing("aur:PhotoId"))?;
        let name = x::text(&mut props, ns::AUR, "Name");
        let created = x::parsed(&mut props, ns::AUR, "Created");
        let overrides = x::parsed_array(&mut props, ns::AUR, "Overrides").unwrap_or_default();
        let added_keyword_ids =
            x::parsed_array(&mut props, ns::AUR, "AddedKeywordIds").unwrap_or_default();
        let copied_from = x::parsed(&mut props, ns::AUR, "CopiedFrom");
        let meta = Metadata::take_from(&mut props);
        Ok(Loaded::Current(Self {
            version_id,
            photo_id,
            name,
            created,
            overrides,
            added_keyword_ids,
            copied_from,
            meta,
            extra: props,
        }))
    }
}
