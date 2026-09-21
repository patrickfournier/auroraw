// SPDX-License-Identifier: GPL-3.0-or-later
//! Walking the workspace: what is recognised, and what is left alone and reported
//! (design note 001 §5.6).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use auroraw_types::{CollectionId, PhotoId, SeriesId, VersionId};

use crate::error::WorkspaceError;
use crate::layout::{
    COLLECTIONS_DIR, EXPORTS, INTERNAL, MARKER, PHOTOS, README, REMOVED, SERIES_DIR, STATE,
    VERSIONS,
};

/// What the scan saw of a file: enough to notice that it changed since the catalogue last read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStat {
    /// The path, relative to the workspace root.
    pub path: PathBuf,
    /// The size in bytes.
    pub size: u64,
    /// The modification time, when the file system reports one.
    pub modified: Option<SystemTime>,
}

/// A recognised file and the identifier its name carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<K> {
    /// The identifier (or identifiers) in the file name.
    pub key: K,
    /// The file.
    pub stat: FileStat,
}

/// Why a file is not recognised. It is never modified or deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignReason {
    /// The name does not match any pattern of the layout (a sync tool's conflicted copy, a note).
    Unrecognised,
    /// The name is right but the file is in another shard than its identifier says.
    WrongShard,
    /// A symbolic link: never followed.
    SymbolicLink,
    /// A folder this version does not know (a newer version's, for instance).
    UnknownFolder,
    /// A file named after a version but not a sidecar: a companion file of a later version.
    Companion,
    /// A name that is not valid text.
    NotText,
}

/// A file or folder left alone, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foreign {
    /// The path, relative to the workspace root.
    pub path: PathBuf,
    /// Why it is not recognised.
    pub reason: ForeignReason,
}

/// The result of walking a workspace.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scan {
    /// The photo sidecars.
    pub photos: Vec<Entry<PhotoId>>,
    /// The version sidecars.
    pub versions: Vec<Entry<(PhotoId, VersionId)>>,
    /// `state/vocabulary.json`, if present.
    pub vocabulary: Option<FileStat>,
    /// `state/sources.json`, if present.
    pub sources: Option<FileStat>,
    /// The collections.
    pub collections: Vec<Entry<CollectionId>>,
    /// The series.
    pub series: Vec<Entry<SeriesId>>,
    /// Everything that is not recognised.
    pub foreign: Vec<Foreign>,
}

fn stat_of(root: &Path, path: &Path, meta: &fs::Metadata) -> FileStat {
    FileStat {
        path: path.strip_prefix(root).unwrap_or(path).to_path_buf(),
        size: meta.len(),
        modified: meta.modified().ok(),
    }
}

fn read_dir(path: &Path) -> Result<Vec<fs::DirEntry>, WorkspaceError> {
    match fs::read_dir(path) {
        Ok(entries) => Ok(entries.filter_map(Result::ok).collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(WorkspaceError::io(path, e)),
    }
}

/// A folder name that is a shard: two lowercase hexadecimal characters.
fn is_shard(name: &str) -> bool {
    name.len() == 2
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

struct Walker<'a> {
    root: &'a Path,
    scan: Scan,
}

impl Walker<'_> {
    fn foreign(&mut self, path: &Path, reason: ForeignReason) {
        let rel = path.strip_prefix(self.root).unwrap_or(path).to_path_buf();
        self.scan.foreign.push(Foreign { path: rel, reason });
    }

    /// The name of an entry as text, or a report that it is not.
    fn name(&mut self, entry: &fs::DirEntry) -> Option<String> {
        match entry.file_name().into_string() {
            Ok(n) => Some(n),
            Err(_) => {
                self.foreign(&entry.path(), ForeignReason::NotText);
                None
            }
        }
    }

    fn file_meta(&mut self, entry: &fs::DirEntry) -> Option<(PathBuf, fs::Metadata, bool)> {
        let path = entry.path();
        let ty = entry.file_type().ok()?;
        if ty.is_symlink() {
            self.foreign(&path, ForeignReason::SymbolicLink);
            return None;
        }
        let meta = entry.metadata().ok()?;
        Some((path, meta, ty.is_dir()))
    }

    fn photos(&mut self) -> Result<(), WorkspaceError> {
        let dir = self.root.join(PHOTOS);
        for shard in read_dir(&dir)? {
            let Some(shard_name) = self.name(&shard) else {
                continue;
            };
            let Some((shard_path, _, is_dir)) = self.file_meta(&shard) else {
                continue;
            };
            if !is_dir || !is_shard(&shard_name) {
                self.foreign(&shard_path, ForeignReason::Unrecognised);
                continue;
            }
            for file in read_dir(&shard_path)? {
                let Some(name) = self.name(&file) else {
                    continue;
                };
                let Some((path, meta, is_dir)) = self.file_meta(&file) else {
                    continue;
                };
                match (!is_dir)
                    .then(|| name.strip_suffix(".xmp"))
                    .flatten()
                    .and_then(|s| s.parse::<PhotoId>().ok())
                {
                    Some(id) if id.shard() == shard_name => {
                        self.scan.photos.push(Entry {
                            key: id,
                            stat: stat_of(self.root, &path, &meta),
                        });
                    }
                    Some(_) => self.foreign(&path, ForeignReason::WrongShard),
                    None => self.foreign(&path, ForeignReason::Unrecognised),
                }
            }
        }
        Ok(())
    }

    fn versions(&mut self) -> Result<(), WorkspaceError> {
        let dir = self.root.join(VERSIONS);
        for shard in read_dir(&dir)? {
            let Some(shard_name) = self.name(&shard) else {
                continue;
            };
            let Some((shard_path, _, is_dir)) = self.file_meta(&shard) else {
                continue;
            };
            if !is_dir || !is_shard(&shard_name) {
                self.foreign(&shard_path, ForeignReason::Unrecognised);
                continue;
            }
            for file in read_dir(&shard_path)? {
                let Some(name) = self.name(&file) else {
                    continue;
                };
                let Some((path, meta, is_dir)) = self.file_meta(&file) else {
                    continue;
                };
                if is_dir {
                    self.foreign(&path, ForeignReason::Unrecognised);
                    continue;
                }
                let mut parts = name.splitn(3, '.');
                let ids = (
                    parts.next().and_then(|p| p.parse::<PhotoId>().ok()),
                    parts.next().and_then(|v| v.parse::<VersionId>().ok()),
                    parts.next(),
                );
                match ids {
                    (Some(photo), Some(version), Some("xmp")) if photo.shard() == shard_name => {
                        self.scan.versions.push(Entry {
                            key: (photo, version),
                            stat: stat_of(self.root, &path, &meta),
                        });
                    }
                    (Some(photo), Some(_), Some("xmp")) if photo.shard() != shard_name => {
                        self.foreign(&path, ForeignReason::WrongShard);
                    }
                    (Some(_), Some(_), Some(_)) => self.foreign(&path, ForeignReason::Companion),
                    _ => self.foreign(&path, ForeignReason::Unrecognised),
                }
            }
        }
        Ok(())
    }

    fn state(&mut self) -> Result<(), WorkspaceError> {
        let dir = self.root.join(STATE);
        for entry in read_dir(&dir)? {
            let Some(name) = self.name(&entry) else {
                continue;
            };
            let Some((path, meta, is_dir)) = self.file_meta(&entry) else {
                continue;
            };
            match (name.as_str(), is_dir) {
                ("vocabulary.json", false) => {
                    self.scan.vocabulary = Some(stat_of(self.root, &path, &meta))
                }
                ("sources.json", false) => {
                    self.scan.sources = Some(stat_of(self.root, &path, &meta))
                }
                (COLLECTIONS_DIR, true) => self.collections(&path)?,
                (SERIES_DIR, true) => self.series(&path)?,
                (_, true) => self.foreign(&path, ForeignReason::UnknownFolder),
                (_, false) => self.foreign(&path, ForeignReason::Unrecognised),
            }
        }
        Ok(())
    }

    fn collections(&mut self, dir: &Path) -> Result<(), WorkspaceError> {
        for file in read_dir(dir)? {
            let Some(name) = self.name(&file) else {
                continue;
            };
            let Some((path, meta, is_dir)) = self.file_meta(&file) else {
                continue;
            };
            match (!is_dir)
                .then(|| name.strip_suffix(".json"))
                .flatten()
                .and_then(|s| s.parse::<CollectionId>().ok())
            {
                Some(id) => self.scan.collections.push(Entry {
                    key: id,
                    stat: stat_of(self.root, &path, &meta),
                }),
                None => self.foreign(&path, ForeignReason::Unrecognised),
            }
        }
        Ok(())
    }

    fn series(&mut self, dir: &Path) -> Result<(), WorkspaceError> {
        for shard in read_dir(dir)? {
            let Some(shard_name) = self.name(&shard) else {
                continue;
            };
            let Some((shard_path, _, is_dir)) = self.file_meta(&shard) else {
                continue;
            };
            if !is_dir || !is_shard(&shard_name) {
                self.foreign(&shard_path, ForeignReason::Unrecognised);
                continue;
            }
            for file in read_dir(&shard_path)? {
                let Some(name) = self.name(&file) else {
                    continue;
                };
                let Some((path, meta, is_dir)) = self.file_meta(&file) else {
                    continue;
                };
                match (!is_dir)
                    .then(|| name.strip_suffix(".json"))
                    .flatten()
                    .and_then(|s| s.parse::<SeriesId>().ok())
                {
                    Some(id) if id.to_string().starts_with(&shard_name) => {
                        self.scan.series.push(Entry {
                            key: id,
                            stat: stat_of(self.root, &path, &meta),
                        });
                    }
                    Some(_) => self.foreign(&path, ForeignReason::WrongShard),
                    None => self.foreign(&path, ForeignReason::Unrecognised),
                }
            }
        }
        Ok(())
    }

    fn root_entries(&mut self) -> Result<(), WorkspaceError> {
        for entry in read_dir(self.root)? {
            let Some(name) = self.name(&entry) else {
                continue;
            };
            match name.as_str() {
                MARKER | README | INTERNAL | EXPORTS | REMOVED | PHOTOS | VERSIONS | STATE => {}
                _ => {
                    let path = entry.path();
                    let reason = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        ForeignReason::UnknownFolder
                    } else {
                        ForeignReason::Unrecognised
                    };
                    self.foreign(&path, reason);
                }
            }
        }
        Ok(())
    }
}

/// Walks the workspace once and classifies everything in it.
pub(crate) fn scan(root: &Path) -> Result<Scan, WorkspaceError> {
    let mut w = Walker {
        root,
        scan: Scan::default(),
    };
    w.root_entries()?;
    w.photos()?;
    w.versions()?;
    w.state()?;
    Ok(w.scan)
}
