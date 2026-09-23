// SPDX-License-Identifier: GPL-3.0-or-later
//! The catalogue's sources as a person manages them (M1 plan, workflow revision D-090..D-092):
//! listing them with what they hold, deciding whether a folder is already covered by one (a folder
//! is a source when it is, or lies inside, a registered source), adding one (which scans it and
//! builds the sidecars, copying nothing) and the checks before removing one.

use std::path::{Path, PathBuf};

use auroraw_catalogue::SourceCounts;
use auroraw_plugin_api::source::{Source, SourceState};
use auroraw_sources::filesystem::{FilesystemSource, LOCAL_FOLDER, REMOVABLE_VOLUME};
use auroraw_types::SourceId;

use crate::command::Command;
use crate::coordinator::Outcome;
use crate::error::{EngineError, Result};
use crate::job::JobId;
use crate::{Engine, paths};

/// A registered source, with what a person wants to see of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    /// Its identifier.
    pub id: SourceId,
    /// Its name.
    pub name: String,
    /// `local-folder`, `removable-volume`, or a plugin's kind.
    pub kind: String,
    /// Where it is on this machine.
    pub path: PathBuf,
    /// Whether it can be reached right now.
    pub online: bool,
    /// How many photos have their original in it.
    pub photos: u64,
    /// How many of those carry work a person did (a rating, a flag, keywords, a version).
    pub worked_on: u64,
}

/// A kind of source that can be added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceKind {
    /// Its identifier.
    pub id: String,
    /// A name for a person, in English (the interface translates it).
    pub name: String,
}

/// What adding a folder as a source would run into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddPlan {
    /// Nothing overlaps it.
    Free,
    /// It is inside a source already registered: its photos are there already.
    InsideExisting(SourceInfo),
    /// It contains sources already registered.
    ContainsExisting(Vec<SourceInfo>),
}

/// Adding a folder as a source.
#[derive(Debug, Clone)]
pub struct AddSourceRequest {
    /// The folder (already canonical, see [`crate::paths::resolve`]).
    pub root: PathBuf,
    /// Its name; the folder's own when `None`.
    pub name: Option<String>,
    /// Merge the sources that are inside the folder into the new one (their photos are kept), when
    /// there are any; without it, a folder that contains sources is refused.
    pub merge: bool,
}

/// A source that was added, and the job scanning it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddedSource {
    /// The new source.
    pub source_id: SourceId,
    /// The index job, reported by `Event::IndexPlanned` and `Event::IndexFinished`.
    pub job: JobId,
}

fn canonical(path: &Path) -> PathBuf {
    paths::resolve(&path.to_string_lossy()).unwrap_or_else(|_| path.to_path_buf())
}

impl Engine {
    /// What kinds of source can be added: the folder (local, or a mounted network share) built in;
    /// source plugins will add theirs.
    pub fn source_kinds() -> Vec<SourceKind> {
        vec![SourceKind {
            id: LOCAL_FOLDER.to_string(),
            name: "Folder".to_string(),
        }]
    }

    /// Every registered source, by name.
    pub fn sources(&self) -> Result<Vec<SourceInfo>> {
        let entries = match self.workspace().read_sources()? {
            Some(loaded) => loaded.current().map(|s| s.sources).unwrap_or_default(),
            None => Vec::new(),
        };
        let catalogue = self.read_catalogue()?;
        let mut out = Vec::with_capacity(entries.len());
        for entry in entries {
            let Some(path) = entry.hint.get("path").and_then(|p| p.as_str()) else {
                continue;
            };
            let path = PathBuf::from(path);
            let SourceCounts { photos, worked_on } = catalogue.source_counts(&entry.id)?;
            out.push(SourceInfo {
                id: entry.id,
                name: entry.name,
                kind: entry.kind,
                online: FilesystemSource::new(path.clone()).state() == SourceState::Online,
                path,
                photos,
                worked_on,
            });
        }
        out.sort_by_key(|s| s.name.to_lowercase());
        Ok(out)
    }

    /// The source that covers `path`, when it equals or lies inside a registered source, with the
    /// path relative to that source. A photo's location is always this pair.
    pub fn covering_source(&self, path: &Path) -> Result<Option<(SourceInfo, PathBuf)>> {
        let path = canonical(path);
        Ok(self.sources()?.into_iter().find_map(|source| {
            paths::relative_to(&path, &canonical(&source.path)).map(|relative| (source, relative))
        }))
    }

    /// What adding `root` as a source would run into (checked before anything is written).
    pub fn plan_add_source(&self, root: &Path) -> Result<AddPlan> {
        let root = canonical(root);
        let sources = self.sources()?;
        if let Some(inside) = sources
            .iter()
            .find(|s| paths::is_inside(&root, &canonical(&s.path)))
        {
            return Ok(AddPlan::InsideExisting(inside.clone()));
        }
        let contained: Vec<SourceInfo> = sources
            .into_iter()
            .filter(|s| paths::is_inside(&canonical(&s.path), &root))
            .collect();
        Ok(if contained.is_empty() {
            AddPlan::Free
        } else {
            AddPlan::ContainsExisting(contained)
        })
    }

    /// Registers a folder as a source and starts scanning it in the background: nothing is copied
    /// (spec §5.1); every photo file becomes a photo with a sidecar holding what the file says
    /// about itself. Refused when the folder is inside a registered source, or contains some.
    pub fn add_source(&self, request: AddSourceRequest) -> Result<AddedSource> {
        let root = canonical(&request.root);
        if !root.is_dir() {
            return Err(EngineError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{} is not a folder", root.display()),
            )));
        }
        let mut merged = Vec::new();
        match self.plan_add_source(&root)? {
            AddPlan::Free => {}
            AddPlan::InsideExisting(source) => {
                return Err(EngineError::AlreadyCovered(source.name));
            }
            AddPlan::ContainsExisting(sources) if request.merge => {
                merged = sources.iter().map(|s| s.id).collect();
            }
            AddPlan::ContainsExisting(sources) => {
                let names: Vec<String> =
                    sources.iter().map(|s| format!("\"{}\"", s.name)).collect();
                return Err(EngineError::ContainsSources(names.join(", ")));
            }
        }
        let removable = Self::removable_volumes()
            .iter()
            .any(|v| canonical(&v.mount_point) == root);
        let name = request
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| {
                root.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.display().to_string())
            });
        let source_id = match self.submit_and_wait(Command::AddSource {
            name,
            root,
            kind: if removable {
                REMOVABLE_VOLUME
            } else {
                LOCAL_FOLDER
            }
            .to_string(),
        })? {
            Outcome::SourceAdded(id) => id,
            other => unreachable!("AddSource answers SourceAdded, not {other:?}"),
        };
        match self.submit_and_wait(Command::IndexSource {
            source_id,
            merge: merged,
        })? {
            Outcome::IndexStarted { job } => Ok(AddedSource { source_id, job }),
            other => unreachable!("IndexSource answers IndexStarted, not {other:?}"),
        }
    }

    /// What is in a source, for the confirmation before removing it.
    pub fn source_counts(&self, source_id: SourceId) -> Result<SourceCounts> {
        Ok(self.read_catalogue()?.source_counts(&source_id)?)
    }
}
