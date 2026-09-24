// SPDX-License-Identifier: GPL-3.0-or-later
//! The background worker behind `Command::IndexSource` (M1 plan, workflow revision D-090): adding a
//! folder to the catalogue "in place" copies nothing (spec §5.1); it lists the folder, reads what
//! each photo's file says about itself into its sidecar (D-074), and registers the photo. None of
//! that runs on the coordinator thread; only the catalogue write of a finished photo goes back to
//! it ([`Inbound::Imported`]), exactly as an import does.
//!
//! Files that match a photo removed earlier (its sidecar waits in the workspace's `removed/`) are
//! reported first, and the job pauses for a person's answer: restore the photo, with its ratings,
//! keywords and versions, or add the file as a new one.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use auroraw_catalogue::Catalogue;
use auroraw_format::sidecar::{FileEntry, FileRole, Location, PhotoSidecar};
use auroraw_import::{DiscoveredFile, PairRule, PhotoGroup, pair_files};
use auroraw_plugin_api::source::{Source, SourceState};
use auroraw_sources::filesystem::FilesystemSource;
use auroraw_types::{Fingerprint, PhotoId, SourceId, Timestamp};
use auroraw_workspace::{RemovedPhoto, Workspace};

use crate::command::Command;
use crate::coordinator::Inbound;
use crate::event::Event;
use crate::import_job::{discover_one, stat_of};
use crate::job::{CancelToken, JobId};

pub(crate) struct IndexJob {
    pub job: JobId,
    pub workspace: Arc<Workspace>,
    pub source: FilesystemSource,
    pub source_id: SourceId,
    /// Folders inside this source that are the roots of other sources, relative to it and with
    /// `/` separators: never descended into, so a photo is never registered twice.
    pub skip: Vec<String>,
    /// Sources inside this one to merge into it first, with their folder relative to it.
    pub merge: Vec<(SourceId, String)>,
    pub catalogue_path: PathBuf,
    pub events: mpsc::Sender<Event>,
    pub inbound: mpsc::Sender<Inbound>,
    pub cancel: CancelToken,
    /// The person's answer to a pause (restore or not).
    pub decision: mpsc::Receiver<bool>,
}

pub(crate) fn spawn(job: IndexJob) {
    std::thread::spawn(move || run(job));
}

fn inside_skipped(path: &str, skip: &[String]) -> bool {
    skip.iter()
        .any(|folder| path == folder || path.starts_with(&format!("{folder}/")))
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn file_entry(
    role: FileRole,
    file: &DiscoveredFile,
    fingerprint: Fingerprint,
    source_id: SourceId,
) -> FileEntry {
    let name = file_name(&file.path);
    let format = Path::new(&name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_uppercase());
    FileEntry {
        role,
        name,
        format,
        size: file.size,
        fingerprint,
        hash: None,
        locations: vec![Location {
            source: source_id,
            path: file.path.clone(),
            seen: Some(Timestamp::now()),
            extra: Vec::new(),
        }],
        extra: Vec::new(),
    }
}

/// A photo the job will add: one or two files of the source, their fingerprints, and the
/// metadata read from the original.
struct Candidate {
    group: PhotoGroup,
    original_fingerprint: Fingerprint,
    companion_fingerprint: Option<Fingerprint>,
    metadata: Option<auroraw_format::sidecar::Metadata>,
    removed: Option<usize>,
}

/// Waits for the person's answer, giving up if the job is cancelled or the engine is gone.
fn wait_for_answer(job: &IndexJob) -> Option<bool> {
    loop {
        if job.cancel.is_cancelled() {
            return None;
        }
        match job.decision.recv_timeout(Duration::from_millis(200)) {
            Ok(answer) => return Some(answer),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn run(job: IndexJob) {
    let abort = |reason: String| {
        let _ = job.events.send(Event::IndexAborted {
            job: job.job,
            reason,
        });
        let _ = job.events.send(Event::JobFinished(job.job));
    };
    if job.source.state() != SourceState::Online {
        return abort("the source is not reachable".to_string());
    }
    let entries = match job.source.list() {
        Ok(entries) => entries,
        Err(e) => return abort(format!("the source cannot be listed: {e}")),
    };
    let Ok(catalogue) = Catalogue::open(&job.catalogue_path) else {
        return abort("the catalogue cannot be read".to_string());
    };
    let mut known: HashSet<String> = catalogue
        .known_files_in_source(&job.source_id)
        .map(|files| files.into_iter().map(|(_, path, _)| path).collect())
        .unwrap_or_default();
    // Photos of sources being merged in become this source's own before anything is listed; their
    // new paths are known here, whether or not the catalogue has caught up with them.
    known.extend(merge_sources(&job, &catalogue));

    // Pair by name only (no metadata read for a file the catalogue already has): a group is known
    // when its original is.
    let listed: Vec<DiscoveredFile> = entries
        .iter()
        .filter(|entry| {
            auroraw_imaging::is_photo_file(Path::new(&entry.path))
                && !inside_skipped(&entry.path, &job.skip)
        })
        .map(|entry| DiscoveredFile {
            path: entry.path.clone(),
            size: entry.size,
            capture_time: None,
            camera: None,
        })
        .collect();
    let groups = pair_files(listed, PairRule::Both);
    let (known_groups, mut new_groups): (Vec<_>, Vec<_>) = groups
        .into_iter()
        .partition(|group| known.contains(&group.original.path));
    // By path, case aside: photos enter the catalogue (and get their thumbnails) in the order a
    // person reads a folder, whatever order the source listed them in.
    new_groups.sort_by_cached_key(|group| {
        (
            group.original.path.to_lowercase(),
            group.original.path.clone(),
        )
    });

    // Fingerprints, and which new photos are ones that were removed earlier.
    let removed = job.workspace.removed_photos();
    let mut by_fingerprint: HashMap<String, usize> = HashMap::new();
    for (index, removed_photo) in removed.iter().enumerate() {
        if let Some(original) = removed_photo
            .photo
            .files
            .iter()
            .find(|file| file.role == FileRole::Original)
        {
            by_fingerprint.insert(original.fingerprint.to_string(), index);
        }
    }
    let mut candidates = Vec::with_capacity(new_groups.len());
    let mut failed = 0;
    let mut claimed: HashSet<usize> = HashSet::new();
    for group in new_groups {
        if job.cancel.is_cancelled() {
            let _ = job.events.send(Event::JobCancelled(job.job));
            return;
        }
        let Ok(original_fingerprint) = job.source.fingerprint_of(&group.original.path) else {
            failed += 1;
            continue;
        };
        let companion_fingerprint = group
            .companion
            .as_ref()
            .and_then(|c| job.source.fingerprint_of(&c.path).ok());
        let removed_match = by_fingerprint
            .get(&original_fingerprint.to_string())
            .copied()
            .filter(|index| claimed.insert(*index));
        candidates.push(Candidate {
            group,
            original_fingerprint,
            companion_fingerprint,
            metadata: None,
            removed: removed_match,
        });
    }
    let restorable = candidates.iter().filter(|c| c.removed.is_some()).count();
    let _ = job.events.send(Event::IndexPlanned {
        job: job.job,
        source_id: job.source_id,
        new_files: candidates.len(),
        restorable,
    });
    let restore = if restorable > 0 {
        match wait_for_answer(&job) {
            Some(answer) => answer,
            None => {
                let _ = job.events.send(Event::JobCancelled(job.job));
                return;
            }
        }
    } else {
        false
    };

    let root = job.source.root().to_path_buf();
    let total = candidates.len();
    let (mut added, mut restored) = (0, 0);
    let mut versions_came_back = false;
    for (done, mut candidate) in candidates.into_iter().enumerate() {
        if job.cancel.is_cancelled() {
            let _ = job.events.send(Event::JobCancelled(job.job));
            return;
        }
        let outcome = match candidate.removed.filter(|_| restore) {
            Some(index) => restore_one(&job, &removed[index], &candidate),
            None => {
                let (_, metadata) = discover_one(&root, &candidate.group.original.path);
                candidate.metadata = metadata;
                add_one(&job, candidate)
            }
        };
        match outcome {
            Ok(Landed::Added) => added += 1,
            Ok(Landed::Restored { versions }) => {
                restored += 1;
                versions_came_back |= versions > 0;
            }
            Err(()) => failed += 1,
        }
        let _ = job.events.send(Event::JobProgress {
            job: job.job,
            done: done + 1,
            total,
        });
    }
    if versions_came_back {
        // Restored version sidecars are new to the catalogue: a reconcile applies them.
        let _ = job.inbound.send(Inbound::Command {
            command: Command::Reconcile,
            reply: None,
        });
    }
    let _ = job.inbound.send(Inbound::Report(Event::IndexFinished {
        job: job.job,
        source_id: job.source_id,
        added,
        restored,
        known: known_groups.len(),
        failed,
    }));
    let _ = job
        .inbound
        .send(Inbound::Report(Event::JobFinished(job.job)));
}

/// Moves every photo of the merged sources into this one: each location's source becomes this
/// source and its path gets the merged folder's place in it. Returns the new paths of the photos'
/// primary files. The merged sources' entries are then dropped.
fn merge_sources(job: &IndexJob, catalogue: &Catalogue) -> Vec<String> {
    let mut paths = Vec::new();
    for (inner, prefix) in &job.merge {
        let mut moved = 0;
        for photo_id in catalogue.photos_in_source(inner).unwrap_or_default() {
            let Some(mut photo) = job
                .workspace
                .read_photo(&photo_id)
                .ok()
                .flatten()
                .and_then(|loaded| loaded.current())
            else {
                continue;
            };
            let mut primary = None;
            for file in &mut photo.files {
                for location in &mut file.locations {
                    if location.source == *inner {
                        location.source = job.source_id;
                        location.path = format!("{prefix}/{}", location.path);
                    }
                }
                if primary.is_none()
                    && file.role == FileRole::Original
                    && let Some(location) = file.locations.first()
                {
                    primary = Some((file.name.clone(), file.fingerprint, location.path.clone()));
                }
            }
            if job.workspace.write_photo(&photo).is_err() {
                continue;
            }
            if let Some((filename, fingerprint, path)) = primary {
                let _ = job.inbound.send(Inbound::Relocated {
                    photo_id,
                    source_id: job.source_id,
                    path: path.clone(),
                    filename,
                    fingerprint,
                });
                paths.push(path);
                moved += 1;
            }
        }
        let _ = job.inbound.send(Inbound::SourceGone {
            job: job.job,
            source_id: *inner,
            removed: 0,
            kept: moved,
            announce: false,
        });
    }
    paths
}

enum Landed {
    Added,
    Restored { versions: usize },
}

fn register(job: &IndexJob, photo: PhotoSidecar) -> Result<(), ()> {
    let stat_path = job.workspace.photo_path(&photo.photo_id);
    let stat = match std::fs::metadata(&stat_path) {
        Ok(m) => stat_of(m.len(), m.modified().ok()),
        Err(_) => return Err(()),
    };
    let _ = job.inbound.send(Inbound::Imported {
        photo: Some(Box::new(photo)),
        stat: Some(stat),
        backfill: None,
    });
    Ok(())
}

fn add_one(job: &IndexJob, candidate: Candidate) -> Result<Landed, ()> {
    let mut photo = PhotoSidecar::new(PhotoId::random());
    photo.meta = candidate.metadata.unwrap_or_default();
    photo.imported = Some(Timestamp::now());
    photo.files.push(file_entry(
        FileRole::Original,
        &candidate.group.original,
        candidate.original_fingerprint,
        job.source_id,
    ));
    if let (Some(companion), Some(fingerprint)) =
        (&candidate.group.companion, candidate.companion_fingerprint)
    {
        photo.files.push(file_entry(
            FileRole::Companion,
            companion,
            fingerprint,
            job.source_id,
        ));
    }
    job.workspace.write_photo(&photo).map_err(|_| ())?;
    register(job, photo)?;
    Ok(Landed::Added)
}

/// Puts a removed photo back: its sidecar (ratings, keywords) with its files' locations pointing at
/// this source, its versions from `removed/` with it.
fn restore_one(
    job: &IndexJob,
    removed: &RemovedPhoto,
    candidate: &Candidate,
) -> Result<Landed, ()> {
    let mut photo = removed.photo.clone();
    let found = [
        Some((&candidate.group.original, candidate.original_fingerprint)),
        candidate
            .group
            .companion
            .as_ref()
            .zip(candidate.companion_fingerprint),
    ];
    for file in &mut photo.files {
        // The source it was in is gone: its locations are stale. Where this source has the same
        // file (by fingerprint), that is the new location; where it does not, the file is missing.
        file.locations.clear();
        if let Some((discovered, _)) = found
            .iter()
            .flatten()
            .find(|(_, fingerprint)| *fingerprint == file.fingerprint)
        {
            file.locations.push(Location {
                source: job.source_id,
                path: discovered.path.clone(),
                seen: Some(Timestamp::now()),
                extra: Vec::new(),
            });
        }
    }
    let versions = job
        .workspace
        .restore_photo(removed, &photo)
        .map_err(|_| ())?;
    register(job, photo)?;
    Ok(Landed::Restored { versions })
}
