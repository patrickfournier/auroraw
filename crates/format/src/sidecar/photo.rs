// SPDX-License-Identifier: GPL-3.0-or-later
use auroraw_types::{
    ContentHash, Fingerprint, PhotoId, SchemaVersion, SourceId, Timestamp, VersionId,
};

use super::SidecarError;
use super::extract as x;
use super::metadata::Metadata;
use crate::state::Loaded;
use crate::xmp::{ArrayKind, Item, Property, Value, Xmp, ns};

/// The schema of the Auroraw part of the photo sidecar this version reads and writes.
pub const PHOTO_SCHEMA: u32 = 1;

/// The role of a file of a photo (D-032).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRole {
    /// The file that is developed (the RAW of a RAW+JPEG pair).
    Original,
    /// A companion file (the JPEG of a pair).
    Companion,
    /// A role written by a newer version, kept as written.
    Other(String),
}

impl FileRole {
    fn as_text(&self) -> &str {
        match self {
            Self::Original => "original",
            Self::Companion => "companion",
            Self::Other(s) => s,
        }
    }
}

/// A place where a file was last seen: a source and a path inside it (note 004 §6.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    /// The source.
    pub source: SourceId,
    /// The path relative to the source, with `/` separators.
    pub path: String,
    /// When it was last seen there.
    pub seen: Option<Timestamp>,
    /// Fields this version does not know, kept.
    pub extra: Vec<Property>,
}

/// One file of a photo (note 003 §4.3, note 004 §6.2).
#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    /// Original or companion.
    pub role: FileRole,
    /// The file name.
    pub name: String,
    /// The format, such as `ARW` or `JPEG`.
    pub format: Option<String>,
    /// The size in bytes.
    pub size: u64,
    /// The sampled fingerprint.
    pub fingerprint: Fingerprint,
    /// The whole-file hash, once known.
    pub hash: Option<ContentHash>,
    /// Where the file was last seen.
    pub locations: Vec<Location>,
    /// Fields this version does not know, kept.
    pub extra: Vec<Property>,
}

/// The sidecar of a photo: its metadata, the cache of the original's data, its files.
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoSidecar {
    /// The photo's identifier.
    pub photo_id: PhotoId,
    /// The metadata.
    pub meta: Metadata,
    /// The files of the photo.
    pub files: Vec<FileEntry>,
    /// The version the grid shows.
    pub main_version: Option<VersionId>,
    /// When the photo was first added.
    pub imported: Option<Timestamp>,
    /// Properties this version does not know, kept and written back.
    pub extra: Vec<Property>,
}

impl PhotoSidecar {
    /// A new sidecar for a photo, with no metadata yet.
    pub fn new(photo_id: PhotoId) -> Self {
        Self {
            photo_id,
            meta: Metadata::default(),
            files: Vec::new(),
            main_version: None,
            imported: None,
            extra: Vec::new(),
        }
    }

    /// The XMP model, in the canonical order.
    pub fn to_xmp(&self) -> Xmp {
        let mut props = vec![
            Property::text(ns::AUR, "Schema", PHOTO_SCHEMA.to_string()),
            Property::text(ns::AUR, "PhotoId", self.photo_id.to_string()),
        ];
        props.extend(self.meta.to_properties());
        if !self.files.is_empty() {
            let items = self
                .files
                .iter()
                .map(|f| Item {
                    lang: None,
                    value: Value::Struct(file_fields(f)),
                })
                .collect();
            props.push(Property {
                ns: ns::AUR.into(),
                name: "Files".into(),
                lang: None,
                value: Value::Array(ArrayKind::Seq, items),
            });
        }
        if let Some(v) = self.main_version {
            props.push(Property::text(ns::AUR, "MainVersion", v.to_string()));
        }
        if let Some(t) = self.imported {
            props.push(Property::text(ns::AUR, "Imported", t.to_string()));
        }
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

    /// Reads a photo sidecar. A schema newer than this version knows is reported, not interpreted.
    pub fn from_bytes(bytes: &[u8]) -> Result<Loaded<Self>, SidecarError> {
        Self::from_xmp(Xmp::from_bytes(bytes)?)
    }

    /// Reads a photo sidecar from its XMP model.
    pub fn from_xmp(xmp: Xmp) -> Result<Loaded<Self>, SidecarError> {
        let mut props = xmp.properties;
        let schema = x::parsed::<u32>(&mut props, ns::AUR, "Schema")
            .ok_or(SidecarError::Missing("aur:Schema"))?;
        if schema > PHOTO_SCHEMA {
            return Ok(Loaded::Newer(SchemaVersion(schema)));
        }
        let photo_id = x::parsed::<PhotoId>(&mut props, ns::AUR, "PhotoId")
            .ok_or(SidecarError::Missing("aur:PhotoId"))?;
        let meta = Metadata::take_from(&mut props);
        let files = read_files(&mut props);
        let main_version = x::parsed(&mut props, ns::AUR, "MainVersion");
        let imported = x::parsed(&mut props, ns::AUR, "Imported");
        Ok(Loaded::Current(Self {
            photo_id,
            meta,
            files,
            main_version,
            imported,
            extra: props,
        }))
    }
}

fn file_fields(f: &FileEntry) -> Vec<Property> {
    let mut fields = vec![
        Property::text(ns::AUR, "Role", f.role.as_text()),
        Property::text(ns::AUR, "Name", f.name.clone()),
    ];
    fields.extend(x::opt_text(ns::AUR, "Format", &f.format));
    fields.push(Property::text(ns::AUR, "Size", f.size.to_string()));
    fields.push(Property::text(
        ns::AUR,
        "Fingerprint",
        f.fingerprint.to_string(),
    ));
    if let Some(h) = &f.hash {
        fields.push(Property::text(ns::AUR, "Hash", h.to_string()));
    }
    if !f.locations.is_empty() {
        let items = f
            .locations
            .iter()
            .map(|l| {
                let mut lf = vec![
                    Property::text(ns::AUR, "Source", l.source.to_string()),
                    Property::text(ns::AUR, "Path", l.path.clone()),
                ];
                if let Some(t) = l.seen {
                    lf.push(Property::text(ns::AUR, "Seen", t.to_string()));
                }
                lf.extend(l.extra.iter().cloned());
                Item {
                    lang: None,
                    value: Value::Struct(lf),
                }
            })
            .collect();
        fields.push(Property {
            ns: ns::AUR.into(),
            name: "Locations".into(),
            lang: None,
            value: Value::Array(ArrayKind::Seq, items),
        });
    }
    fields.extend(f.extra.iter().cloned());
    fields
}

/// Reads `aur:Files`. If any entry cannot be understood the whole property is left in place, so
/// it is kept and written back untouched.
fn read_files(props: &mut Vec<Property>) -> Vec<FileEntry> {
    let Some(i) = props.iter().position(|p| p.is(ns::AUR, "Files")) else {
        return Vec::new();
    };
    let backup = props[i].clone();
    let Some(entries) = x::struct_items(props, ns::AUR, "Files") else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for fields in entries {
        match read_file(fields) {
            Some(f) => files.push(f),
            None => {
                props.insert(i.min(props.len()), backup);
                return Vec::new();
            }
        }
    }
    files
}

fn read_file(mut f: Vec<Property>) -> Option<FileEntry> {
    let role = match x::text(&mut f, ns::AUR, "Role")?.as_str() {
        "original" => FileRole::Original,
        "companion" => FileRole::Companion,
        other => FileRole::Other(other.to_string()),
    };
    let name = x::text(&mut f, ns::AUR, "Name")?;
    let format = x::text(&mut f, ns::AUR, "Format");
    let size = x::parsed(&mut f, ns::AUR, "Size")?;
    let fingerprint = x::parsed(&mut f, ns::AUR, "Fingerprint")?;
    let hash = x::parsed(&mut f, ns::AUR, "Hash");
    let mut locations = Vec::new();
    if let Some(items) = x::struct_items(&mut f, ns::AUR, "Locations") {
        for mut l in items {
            locations.push(Location {
                source: x::parsed(&mut l, ns::AUR, "Source")?,
                path: x::text(&mut l, ns::AUR, "Path")?,
                seen: x::parsed(&mut l, ns::AUR, "Seen"),
                extra: l,
            });
        }
    }
    Some(FileEntry {
        role,
        name,
        format,
        size,
        fingerprint,
        hash,
        locations,
        extra: f,
    })
}
