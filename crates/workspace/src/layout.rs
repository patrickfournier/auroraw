// SPDX-License-Identifier: GPL-3.0-or-later
//! Names and paths of the layout (design note 001 §5.1, §5.2).

use std::path::{Path, PathBuf};

use auroraw_types::{CollectionId, PhotoId, SeriesId, VersionId};

/// The layout version this crate reads and writes, written in the marker.
pub const LAYOUT_VERSION: u32 = 1;

pub(crate) const MARKER: &str = "workspace.json";
pub(crate) const README: &str = "README.txt";
pub(crate) const PHOTOS: &str = "photos";
pub(crate) const VERSIONS: &str = "versions";
pub(crate) const STATE: &str = "state";
pub(crate) const COLLECTIONS_DIR: &str = "collections";
pub(crate) const SERIES_DIR: &str = "series";
pub(crate) const EXPORTS: &str = "exports";
pub(crate) const REMOVED: &str = "removed";
pub(crate) const INTERNAL: &str = ".auroraw";
pub(crate) const TMP: &str = "tmp";
pub(crate) const LOCK: &str = "lock";

pub(crate) const README_TEXT: &str = "This folder is an Auroraw workspace.\n\
It holds the sidecars and state files of a catalogue: the truth about your photos' metadata\n\
and edits. Back it up by copying the whole folder. Please do not edit its files by hand.\n\
https://auroraw.org\n";

pub(crate) fn marker(root: &Path) -> PathBuf {
    root.join(MARKER)
}

pub(crate) fn photo(root: &Path, id: &PhotoId) -> PathBuf {
    root.join(PHOTOS).join(id.shard()).join(format!("{id}.xmp"))
}

pub(crate) fn version(root: &Path, photo: &PhotoId, version: &VersionId) -> PathBuf {
    root.join(VERSIONS)
        .join(photo.shard())
        .join(format!("{photo}.{version}.xmp"))
}

pub(crate) fn vocabulary(root: &Path) -> PathBuf {
    root.join(STATE).join("vocabulary.json")
}

pub(crate) fn sources(root: &Path) -> PathBuf {
    root.join(STATE).join("sources.json")
}

pub(crate) fn collection(root: &Path, id: &CollectionId) -> PathBuf {
    root.join(STATE)
        .join(COLLECTIONS_DIR)
        .join(format!("{id}.json"))
}

pub(crate) fn series(root: &Path, id: &SeriesId) -> PathBuf {
    let text = id.to_string();
    root.join(STATE)
        .join(SERIES_DIR)
        .join(&text[..2])
        .join(format!("{text}.json"))
}
