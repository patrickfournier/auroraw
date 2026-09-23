// SPDX-License-Identifier: GPL-3.0-or-later
//! The single writer (architecture §4.2): one thread, owning the only mutable `Catalogue`
//! connection, applying commands strictly one at a time. Everything that changes the workspace or
//! the catalogue goes through here, whichever thread asked for it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::SystemTime;

use auroraw_catalogue::{Catalogue, SidecarStat};
use auroraw_format::sidecar::{FileEntry, FileRole, Location, PhotoSidecar, VersionSidecar};
use auroraw_format::state::{KeywordEntry, SourceEntry, Vocabulary};
use auroraw_import::Profile;
use auroraw_plugin_api::source::{Source, SourceState};
use auroraw_sources::filesystem::FilesystemSource;
use auroraw_sources::relink::{self, FoundFile, KnownFile, ScanOutcome};
use auroraw_types::{ContentHash, KeywordId, PhotoId, SourceId, Timestamp};
use auroraw_workspace::{FileStat, Workspace};

use crate::command::Command;
use crate::error::{EngineError, Result};
use crate::event::Event;
use crate::import_job::{self, ImportJob};
use crate::index_job::{self, IndexJob};
use crate::job::{CancelToken, JobId};
use crate::remove_job::{self, RemoveJob};

/// What a command that waits for its result (`Engine::submit_and_wait`) gets back.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The command applied with nothing more specific to report.
    Applied,
    /// `CreateKeyword`'s new identifier.
    KeywordCreated(KeywordId),
    /// `RenameKeyword`'s background refresh job and how many sidecars it will touch.
    RenameStarted {
        /// The job doing the refresh.
        job: JobId,
        /// How many sidecars it will touch.
        affected: usize,
    },
    /// `AddSource`'s new identifier.
    SourceAdded(SourceId),
    /// `ScanSource`'s report: what the reconcile applied, and what it left for a person or
    /// `AddNewPhotos`.
    Scanned {
        /// Whether the source could be reached at all.
        reachable: bool,
        /// How many files matched exactly what the catalogue expected.
        confirmed: usize,
        /// How many files changed at their known path.
        changed: usize,
        /// How many files were relinked silently.
        relinked: usize,
        /// How many known files were found nowhere.
        missing: usize,
        /// Paths that matched nothing known: offer these to `AddNewPhotos`.
        new: Vec<String>,
        /// How many found files were ambiguous.
        ambiguous: usize,
    },
    /// `AddNewPhotos`'s new identifiers, in the order their paths were given.
    PhotosAdded(Vec<PhotoId>),
    /// `Import`'s background job.
    ImportStarted {
        /// The job doing the import.
        job: JobId,
    },
    /// `IndexSource`'s background job.
    IndexStarted {
        /// The job scanning the source.
        job: JobId,
    },
    /// `RemoveSource`'s background job.
    RemoveStarted {
        /// The job removing the source.
        job: JobId,
    },
}

pub(crate) type Reply = mpsc::Sender<Result<Outcome>>;

pub(crate) enum Inbound {
    /// An event a job wants reported after everything it sent before it has been applied: the
    /// catalogue writes it queued are ahead of it in this queue, so whoever reacts to the event by
    /// reading the catalogue sees them all.
    Report(Event),
    /// A remove job took a photo's sidecar out of the workspace (recoverably): take its row out of
    /// the catalogue.
    Removed { photo_id: PhotoId },
    /// A remove job dropped a source from a photo that has another location: point its row at it.
    Relocated {
        photo_id: PhotoId,
        source_id: SourceId,
        path: String,
        filename: String,
        fingerprint: auroraw_types::Fingerprint,
    },
    /// A remove job is done with a source's photos: take the source itself out.
    SourceGone {
        job: JobId,
        source_id: SourceId,
        removed: usize,
        kept: usize,
    },
    /// The last [`crate::Engine`] handle was dropped: cancel what runs in the background and stop.
    /// (The coordinator holds a sender to its own queue for the jobs it starts, so a closed queue
    /// never signals the end by itself.)
    Stop,
    Command {
        command: Command,
        reply: Option<Reply>,
    },
    /// A background job finished refreshing one sidecar: apply the same change to the
    /// catalogue's row, on the coordinator thread, like everything else.
    Refreshed {
        photo: Box<PhotoSidecar>,
        stat: SidecarStat,
        main_version: Option<Box<VersionSidecar>>,
    },
    /// An import job (`crate::import_job`) landed one photo, or discovered (while checking a
    /// fingerprint match) the whole-file hash of a photo that only had a fingerprint recorded:
    /// `photo` and `stat` are `None` for a check that found nothing to register, `backfill` is
    /// `Some` whenever a hash was discovered either way.
    Imported {
        photo: Option<Box<PhotoSidecar>>,
        stat: Option<SidecarStat>,
        backfill: Option<(PhotoId, ContentHash)>,
    },
}

fn sidecar_stat(stat: &FileStat) -> SidecarStat {
    stat_from(stat.size, stat.modified)
}

fn stat_from(size: u64, modified: Option<SystemTime>) -> SidecarStat {
    let modified = modified
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    SidecarStat { size, modified }
}

pub(crate) struct Coordinator {
    workspace: Arc<Workspace>,
    catalogue: Catalogue,
    events: mpsc::Sender<Event>,
    inbound: mpsc::Sender<Inbound>,
    jobs: HashMap<JobId, CancelToken>,
    /// Where the answer to an index job's pause goes.
    index_decisions: HashMap<JobId, mpsc::Sender<bool>>,
    next_job: u64,
}

impl Coordinator {
    pub(crate) fn new(
        workspace: Arc<Workspace>,
        catalogue: Catalogue,
        events: mpsc::Sender<Event>,
        inbound: mpsc::Sender<Inbound>,
    ) -> Self {
        Self {
            workspace,
            catalogue,
            events,
            inbound,
            jobs: HashMap::new(),
            index_decisions: HashMap::new(),
            next_job: 0,
        }
    }

    /// Runs until the channel closes (every `Engine` handle and every job's sender dropped).
    pub(crate) fn run(mut self, rx: mpsc::Receiver<Inbound>) {
        for message in rx {
            match message {
                Inbound::Stop => {
                    for token in self.jobs.values() {
                        token.cancel();
                    }
                    break;
                }
                Inbound::Report(event) => {
                    let _ = self.events.send(event);
                }
                Inbound::Removed { photo_id } => {
                    let _ = self.catalogue.remove_photo(&photo_id);
                }
                Inbound::Relocated {
                    photo_id,
                    source_id,
                    path,
                    filename,
                    fingerprint,
                } => {
                    let _ = self.catalogue.apply_relink(
                        &photo_id,
                        &source_id,
                        &path,
                        &filename,
                        &fingerprint,
                    );
                }
                Inbound::SourceGone {
                    job,
                    source_id,
                    removed,
                    kept,
                } => self.finish_remove_source(job, source_id, removed, kept),
                Inbound::Command { command, reply } => self.handle_command(command, reply),
                Inbound::Refreshed {
                    photo,
                    stat,
                    main_version,
                } => self.handle_refreshed(*photo, stat, main_version.as_deref()),
                Inbound::Imported {
                    photo,
                    stat,
                    backfill,
                } => self.handle_imported(photo.map(|p| *p), stat, backfill),
            }
        }
        let _ = self.events.send(Event::Stopped);
    }

    fn handle_command(&mut self, command: Command, reply: Option<Reply>) {
        let result = self.apply(command.clone());
        match result {
            Ok(outcome) => {
                let _ = self.events.send(Event::Applied(command));
                if let Some(reply) = reply {
                    let _ = reply.send(Ok(outcome));
                }
            }
            Err(e) => {
                let _ = self.events.send(Event::Failed {
                    command,
                    error: e.to_string(),
                });
                if let Some(reply) = reply {
                    let _ = reply.send(Err(e));
                }
            }
        }
    }

    fn apply(&mut self, command: Command) -> Result<Outcome> {
        match command {
            Command::Rebuild => self.rebuild(),
            Command::Reconcile => self.reconcile(),
            Command::SetRating { photo_id, rating } => {
                self.edit_photo(photo_id, |m| m.rating = Some(rating))
            }
            Command::SetFlag { photo_id, flag } => self.edit_photo(photo_id, |m| m.flag = flag),
            Command::AddKeyword {
                photo_id,
                keyword_id,
            } => self.edit_keywords(photo_id, keyword_id, true),
            Command::RemoveKeyword {
                photo_id,
                keyword_id,
            } => self.edit_keywords(photo_id, keyword_id, false),
            Command::CreateKeyword { name, parent } => self.create_keyword(name, parent),
            Command::RenameKeyword {
                keyword_id,
                new_name,
            } => self.rename_keyword(keyword_id, new_name),
            Command::CancelJob { job_id } => self.cancel_job(job_id),
            Command::AddSource { name, root, kind } => self.add_source(name, root, kind),
            Command::ScanSource { source_id } => self.scan_source(source_id),
            Command::AddNewPhotos { source_id, paths } => self.add_new_photos(source_id, paths),
            Command::IndexSource { source_id, merge } => self.start_index(source_id, merge),
            Command::ContinueIndex { job_id, restore } => {
                if let Some(answer) = self.index_decisions.get(&job_id) {
                    let _ = answer.send(restore);
                }
                Ok(Outcome::Applied)
            }
            Command::RemoveSource { source_id } => self.start_remove(source_id),
            Command::Import {
                source_id,
                destination_source_id,
                profile,
                shoot,
                backup_roots,
                state_path,
            } => self.start_import(
                source_id,
                destination_source_id,
                profile,
                shoot,
                backup_roots,
                state_path,
            ),
        }
    }

    fn read_photo(&self, id: &PhotoId) -> Result<(PhotoSidecar, SidecarStat)> {
        let loaded = self
            .workspace
            .read_photo(id)?
            .ok_or_else(|| EngineError::NotFound {
                kind: "photo",
                id: id.to_string(),
            })?;
        let photo = loaded.current().ok_or_else(|| EngineError::NotFound {
            kind: "photo (newer schema)",
            id: id.to_string(),
        })?;
        let meta = std::fs::metadata(self.workspace.photo_path(id)).map_err(EngineError::Io)?;
        Ok((photo, stat_from(meta.len(), meta.modified().ok())))
    }

    fn main_version_of(&self, photo: &PhotoSidecar) -> Result<Option<VersionSidecar>> {
        let Some(version_id) = photo.main_version else {
            return Ok(None);
        };
        let loaded = self.workspace.read_version(&photo.photo_id, &version_id)?;
        Ok(loaded.and_then(|l| l.current()))
    }

    fn edit_photo(
        &mut self,
        photo_id: PhotoId,
        edit: impl FnOnce(&mut auroraw_format::sidecar::Metadata),
    ) -> Result<Outcome> {
        let (mut photo, _) = self.read_photo(&photo_id)?;
        edit(&mut photo.meta);
        self.workspace.write_photo(&photo)?;
        let (photo, stat) = self.read_photo(&photo_id)?; // the just-written size and time
        let main_version = self.main_version_of(&photo)?;
        self.catalogue
            .apply_photo_metadata(&photo, stat, main_version.as_ref())?;
        let _ = self.events.send(Event::PhotoChanged(photo_id));
        Ok(Outcome::Applied)
    }

    fn edit_keywords(
        &mut self,
        photo_id: PhotoId,
        keyword_id: KeywordId,
        add: bool,
    ) -> Result<Outcome> {
        let (mut photo, _) = self.read_photo(&photo_id)?;
        if add {
            if !photo.meta.keyword_ids.contains(&keyword_id) {
                let path = self.keyword_path(&keyword_id)?;
                photo.meta.push_keyword(keyword_id, path);
            }
        } else if let Some(i) = photo.meta.keyword_ids.iter().position(|k| *k == keyword_id) {
            photo.meta.keyword_ids.remove(i);
            if i < photo.meta.keyword_paths.len() {
                photo.meta.keyword_paths.remove(i);
            }
        }
        self.workspace.write_photo(&photo)?;
        let (photo, stat) = self.read_photo(&photo_id)?;
        let main_version = self.main_version_of(&photo)?;
        self.catalogue
            .apply_photo_metadata(&photo, stat, main_version.as_ref())?;
        let _ = self.events.send(Event::PhotoChanged(photo_id));
        Ok(Outcome::Applied)
    }

    fn keyword_path(&self, id: &KeywordId) -> Result<String> {
        let vocabulary = self.read_vocabulary()?;
        let paths = auroraw_catalogue::keyword_paths(&vocabulary.keywords);
        paths.get(id).cloned().ok_or_else(|| EngineError::NotFound {
            kind: "keyword",
            id: id.to_string(),
        })
    }

    fn read_vocabulary(&self) -> Result<auroraw_format::state::Vocabulary> {
        match self.workspace.read_vocabulary()? {
            Some(loaded) => loaded.current().ok_or_else(|| EngineError::NotFound {
                kind: "vocabulary (newer schema)",
                id: String::new(),
            }),
            None => Ok(auroraw_format::state::Vocabulary {
                updated: Timestamp::now(),
                keywords: Vec::new(),
                extra: Default::default(),
            }),
        }
    }

    fn create_keyword(&mut self, name: String, parent: Option<KeywordId>) -> Result<Outcome> {
        let mut vocabulary = self.read_vocabulary()?;
        let id = KeywordId::random();
        vocabulary.keywords.push(KeywordEntry {
            id,
            name,
            parent,
            synonyms: Vec::new(),
            export: true,
            extra: Default::default(),
        });
        vocabulary.updated = Timestamp::now();
        self.workspace.write_vocabulary(&vocabulary)?;
        let path = auroraw_catalogue::keyword_paths(&vocabulary.keywords)
            .remove(&id)
            .unwrap_or_default();
        let entry = vocabulary
            .keywords
            .into_iter()
            .find(|k| k.id == id)
            .expect("just inserted");
        self.catalogue.apply_keyword(&entry, &path)?;
        let _ = self.events.send(Event::KeywordCreated(id));
        Ok(Outcome::KeywordCreated(id))
    }

    fn rename_keyword(&mut self, keyword_id: KeywordId, new_name: String) -> Result<Outcome> {
        let mut vocabulary = self.read_vocabulary()?;
        if !vocabulary.keywords.iter().any(|k| k.id == keyword_id) {
            return Err(EngineError::NotFound {
                kind: "keyword",
                id: keyword_id.to_string(),
            });
        }
        if let Some(entry) = vocabulary.keywords.iter_mut().find(|k| k.id == keyword_id) {
            entry.name = new_name;
        }
        vocabulary.updated = Timestamp::now();
        self.workspace.write_vocabulary(&vocabulary)?;

        let affected = descendants_of(&vocabulary.keywords, keyword_id);
        let paths = auroraw_catalogue::keyword_paths(&vocabulary.keywords);
        for id in &affected {
            if let (Some(entry), Some(path)) = (
                vocabulary.keywords.iter().find(|k| k.id == *id),
                paths.get(id),
            ) {
                self.catalogue.apply_keyword(entry, path)?;
            }
        }

        let photo_ids = self.catalogue.photos_with_keywords(&affected)?;
        let job = self.spawn_job();
        crate::refresh::spawn(
            job,
            self.workspace.clone(),
            photo_ids.clone(),
            paths,
            self.events.clone(),
            self.inbound.clone(),
            self.jobs[&job].clone(),
        );
        let _ = self.events.send(Event::KeywordRenamed {
            keyword_id,
            job,
            affected: photo_ids.len(),
        });
        Ok(Outcome::RenameStarted {
            job,
            affected: photo_ids.len(),
        })
    }

    fn spawn_job(&mut self) -> JobId {
        let id = JobId(self.next_job);
        self.next_job += 1;
        self.jobs.insert(id, CancelToken::new());
        id
    }

    fn cancel_job(&mut self, job_id: JobId) -> Result<Outcome> {
        if let Some(token) = self.jobs.get(&job_id) {
            token.cancel();
        }
        Ok(Outcome::Applied)
    }

    fn handle_refreshed(
        &mut self,
        photo: PhotoSidecar,
        stat: SidecarStat,
        main_version: Option<&VersionSidecar>,
    ) {
        let id = photo.photo_id;
        if self
            .catalogue
            .apply_photo_metadata(&photo, stat, main_version)
            .is_ok()
        {
            let _ = self.events.send(Event::PhotoChanged(id));
        }
        // A failure here (the photo vanished mid-refresh) is not fatal: reconcile will catch it.
    }

    fn rebuild(&mut self) -> Result<Outcome> {
        let scan = self.workspace.scan()?;
        let photos: Vec<(PhotoSidecar, SidecarStat)> = scan
            .photos
            .iter()
            .filter_map(|e| {
                self.workspace
                    .read_photo(&e.key)
                    .ok()
                    .flatten()
                    .and_then(|l| l.current())
                    .map(|p| (p, sidecar_stat(&e.stat)))
            })
            .collect();
        let versions: Vec<(VersionSidecar, SidecarStat)> = scan
            .versions
            .iter()
            .filter_map(|e| {
                let (photo, version) = e.key;
                self.workspace
                    .read_version(&photo, &version)
                    .ok()
                    .flatten()
                    .and_then(|l| l.current())
                    .map(|v| (v, sidecar_stat(&e.stat)))
            })
            .collect();
        let vocabulary = self
            .workspace
            .read_vocabulary()?
            .and_then(|l| l.current())
            .map(|v| v.keywords)
            .unwrap_or_default();
        let sources = self
            .workspace
            .read_sources()?
            .and_then(|l| l.current())
            .map(|s| s.sources)
            .unwrap_or_default();
        let collections: Vec<_> = scan
            .collections
            .iter()
            .filter_map(|e| {
                self.workspace
                    .read_collection(&e.key)
                    .ok()
                    .flatten()
                    .and_then(|l| l.current())
                    .map(|c| (c, sidecar_stat(&e.stat)))
            })
            .collect();
        let series: Vec<_> = scan
            .series
            .iter()
            .filter_map(|e| {
                self.workspace
                    .read_series(&e.key)
                    .ok()
                    .flatten()
                    .and_then(|l| l.current())
                    .map(|s| (s, sidecar_stat(&e.stat)))
            })
            .collect();

        let input = auroraw_catalogue::RebuildInput {
            photos: &photos,
            versions: &versions,
            vocabulary: &vocabulary,
            sources: &sources,
            collections: &collections,
            series: &series,
        };
        // The engine always opens a catalogue file (Engine::create/open never uses an in-memory
        // one: rebuild_to_file needs somewhere to atomically replace).
        let path = self
            .catalogue
            .path()
            .expect("the engine's catalogue is always a file")
            .to_path_buf();
        let workspace_id = self.workspace.workspace_id();
        let photo_count = photos.len();
        let version_count = versions.len();
        // Windows refuses to replace a file that is still open (unlike POSIX, where renaming
        // over an open file just works): close our own connection to `path` before
        // `rebuild_to_file` renames the freshly built catalogue over it.
        self.catalogue = Catalogue::open_in_memory(workspace_id)?;
        self.catalogue = auroraw_catalogue::rebuild_to_file(&path, workspace_id, &input)?;
        let _ = self.events.send(Event::RebuildFinished {
            photos: photo_count,
            versions: version_count,
        });
        Ok(Outcome::Applied)
    }

    fn reconcile(&mut self) -> Result<Outcome> {
        let scan = self.workspace.scan()?;
        let current: Vec<(PhotoId, SidecarStat)> = scan
            .photos
            .iter()
            .map(|e| (e.key, sidecar_stat(&e.stat)))
            .collect();
        let report = self.catalogue.reconcile_photos(&current)?;
        let mut changed = 0;
        for id in &report.changed {
            if let Ok((photo, stat)) = self.read_photo(id) {
                let main_version = self.main_version_of(&photo).ok().flatten();
                if self
                    .catalogue
                    .apply_photo_metadata(&photo, stat, main_version.as_ref())
                    .is_ok()
                {
                    changed += 1;
                    let _ = self.events.send(Event::PhotoChanged(*id));
                }
            }
        }
        let _ = self.events.send(Event::ReconcileFinished {
            changed_photos: changed,
            removed_photos: report.removed.len(),
        });
        Ok(Outcome::Applied)
    }

    fn read_sources(&self) -> Result<auroraw_format::state::Sources> {
        match self.workspace.read_sources()? {
            Some(loaded) => loaded.current().ok_or_else(|| EngineError::NotFound {
                kind: "sources (newer schema)",
                id: String::new(),
            }),
            None => Ok(auroraw_format::state::Sources {
                updated: Timestamp::now(),
                sources: Vec::new(),
                extra: Default::default(),
            }),
        }
    }

    fn source_entry(&self, source_id: &SourceId) -> Result<SourceEntry> {
        self.read_sources()?
            .sources
            .into_iter()
            .find(|s| s.id == *source_id)
            .ok_or_else(|| EngineError::NotFound {
                kind: "source",
                id: source_id.to_string(),
            })
    }

    /// The real path on this machine (design note 002 §6.6: never stored anywhere but the
    /// workspace's `hint` and, once registered, the catalogue).
    fn source_root(entry: &SourceEntry) -> Result<PathBuf> {
        entry
            .hint
            .get("path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .ok_or_else(|| EngineError::NotFound {
                kind: "source path (hint)",
                id: entry.id.to_string(),
            })
    }

    /// Every M1 source is a folder here or a removable volume's mount point, read the same way
    /// (`sources::filesystem`).
    fn open_source(entry: &SourceEntry) -> Result<FilesystemSource> {
        Ok(FilesystemSource::new(Self::source_root(entry)?))
    }

    fn add_source(&mut self, name: String, root: PathBuf, kind: String) -> Result<Outcome> {
        let mut sources = self.read_sources()?;
        let id = SourceId::random();
        let mut hint = serde_json::Map::new();
        hint.insert(
            "path".to_string(),
            serde_json::Value::String(root.to_string_lossy().into_owned()),
        );
        let entry = SourceEntry {
            id,
            kind,
            name,
            hint,
            ignore: Vec::new(),
            config: Default::default(),
            extra: Default::default(),
        };
        sources.sources.push(entry.clone());
        sources.updated = Timestamp::now();
        self.workspace.write_sources(&sources)?;
        self.catalogue.apply_source(&entry)?;
        let _ = self.events.send(Event::SourceAdded(id));
        Ok(Outcome::SourceAdded(id))
    }

    fn scan_source(&mut self, source_id: SourceId) -> Result<Outcome> {
        let entry = self.source_entry(&source_id)?;
        let source = Self::open_source(&entry)?;
        if source.state() != SourceState::Online {
            let _ = self.events.send(Event::SourceScanned {
                source_id,
                reachable: false,
                confirmed: 0,
                changed: 0,
                relinked: 0,
                missing: 0,
                new: Vec::new(),
                ambiguous: 0,
            });
            return Ok(Outcome::Scanned {
                reachable: false,
                confirmed: 0,
                changed: 0,
                relinked: 0,
                missing: 0,
                new: Vec::new(),
                ambiguous: 0,
            });
        }
        let found: Vec<FoundFile> = source.scan()?;
        let known: Vec<KnownFile> = self
            .catalogue
            .known_files_in_source(&source_id)?
            .into_iter()
            .map(|(photo_id, path, fingerprint)| KnownFile {
                photo_id,
                path,
                fingerprint,
            })
            .collect();
        let outcomes = relink::reconcile(&found, &known);

        let (mut confirmed, mut changed, mut relinked, mut missing, mut ambiguous) =
            (0, 0, 0, 0, 0);
        let mut new = Vec::new();
        for outcome in outcomes {
            match outcome {
                ScanOutcome::Confirmed { photo_id } => {
                    self.catalogue.mark_original_changed(&photo_id, false)?;
                    self.catalogue.mark_missing(&photo_id, false)?;
                    confirmed += 1;
                }
                ScanOutcome::OriginalChanged { photo_id, .. } => {
                    self.catalogue.mark_original_changed(&photo_id, true)?;
                    changed += 1;
                }
                ScanOutcome::Relinked { photo_id, to, .. } => {
                    let filename = to.rsplit('/').next().unwrap_or(&to).to_string();
                    let fingerprint = found
                        .iter()
                        .find(|f| f.path == to)
                        .map(|f| f.fingerprint)
                        .expect("the relink target was just found");
                    self.catalogue.apply_relink(
                        &photo_id,
                        &source_id,
                        &to,
                        &filename,
                        &fingerprint,
                    )?;
                    relinked += 1;
                }
                ScanOutcome::Missing { photo_id, .. } => {
                    self.catalogue.mark_missing(&photo_id, true)?;
                    missing += 1;
                }
                ScanOutcome::New { path } => new.push(path),
                ScanOutcome::Ambiguous { .. } => ambiguous += 1,
            }
        }
        let _ = self.events.send(Event::SourceScanned {
            source_id,
            reachable: true,
            confirmed,
            changed,
            relinked,
            missing,
            new: new.clone(),
            ambiguous,
        });
        Ok(Outcome::Scanned {
            reachable: true,
            confirmed,
            changed,
            relinked,
            missing,
            new,
            ambiguous,
        })
    }

    fn add_new_photos(&mut self, source_id: SourceId, paths: Vec<String>) -> Result<Outcome> {
        let entry = self.source_entry(&source_id)?;
        let source = Self::open_source(&entry)?;
        let mut added = Vec::new();
        for path in paths {
            let fingerprint = source.fingerprint_of(&path)?;
            let stat = source.stat(&path)?;
            let filename = path.rsplit('/').next().unwrap_or(&path).to_string();
            let photo_id = PhotoId::random();
            let mut photo = PhotoSidecar::new(photo_id);
            photo.imported = Some(Timestamp::now());
            photo.files.push(FileEntry {
                role: FileRole::Original,
                name: filename,
                format: None,
                size: stat.size,
                fingerprint,
                hash: None,
                locations: vec![Location {
                    source: source_id,
                    path: path.clone(),
                    seen: Some(Timestamp::now()),
                    extra: Vec::new(),
                }],
                extra: Vec::new(),
            });
            self.workspace.write_photo(&photo)?;
            let (photo, stat) = self.read_photo(&photo_id)?;
            self.catalogue.apply_new_photo(&photo, stat)?;
            added.push(photo_id);
        }
        let _ = self.events.send(Event::PhotosAdded {
            source_id,
            count: added.len(),
        });
        Ok(Outcome::PhotosAdded(added))
    }

    /// Resolves `path` (`"Family|Wedding"`) against `vocabulary`, creating whatever segment does
    /// not exist yet, and returns the leaf's identifier. Mutates `vocabulary` in place; the
    /// caller writes it and applies every touched keyword to the catalogue once, after resolving
    /// every path a profile's metadata template names, not once per segment.
    fn resolve_keyword_path(vocabulary: &mut Vocabulary, path: &str) -> Option<KeywordId> {
        let mut parent: Option<KeywordId> = None;
        for segment in path.split('|').filter(|s| !s.is_empty()) {
            let existing = vocabulary
                .keywords
                .iter()
                .find(|k| k.name == segment && k.parent == parent)
                .map(|k| k.id);
            parent = Some(existing.unwrap_or_else(|| {
                let id = KeywordId::random();
                vocabulary.keywords.push(KeywordEntry {
                    id,
                    name: segment.to_string(),
                    parent,
                    synonyms: Vec::new(),
                    export: true,
                    extra: Default::default(),
                });
                id
            }));
        }
        parent
    }

    fn start_import(
        &mut self,
        source_id: SourceId,
        destination_source_id: SourceId,
        profile: Profile,
        shoot: Option<String>,
        backup_roots: Vec<PathBuf>,
        state_path: PathBuf,
    ) -> Result<Outcome> {
        let entry = self.source_entry(&source_id)?;
        let source = Self::open_source(&entry)?;
        let dest_entry = self.source_entry(&destination_source_id)?;
        let dest_root = Self::source_root(&dest_entry)?;

        // Every keyword the profile's template names is resolved (and created if needed) once,
        // here, before the background job starts: per-photo would mean racing to create "the
        // same" new keyword from several photos at once, and the vocabulary is small enough that
        // there is no cost to doing it all up front.
        let mut vocabulary = self.read_vocabulary()?;
        let mut template_keywords = Vec::new();
        for path in &profile.metadata_template.keyword_paths {
            if let Some(id) = Self::resolve_keyword_path(&mut vocabulary, path) {
                template_keywords.push((id, path.clone()));
            }
        }
        vocabulary.updated = Timestamp::now();
        self.workspace.write_vocabulary(&vocabulary)?;
        let paths = auroraw_catalogue::keyword_paths(&vocabulary.keywords);
        for (id, _) in &template_keywords {
            if let (Some(entry), Some(path)) = (
                vocabulary.keywords.iter().find(|k| k.id == *id),
                paths.get(id),
            ) {
                self.catalogue.apply_keyword(entry, path)?;
            }
        }

        let job = self.spawn_job();
        let catalogue_path = self
            .catalogue
            .path()
            .expect("the engine's catalogue is always a file")
            .to_path_buf();
        import_job::spawn(ImportJob {
            job,
            workspace: self.workspace.clone(),
            source,
            dest_root,
            dest_source_id: destination_source_id,
            profile,
            shoot,
            backup_roots,
            state_path,
            catalogue_path,
            template_keywords,
            events: self.events.clone(),
            inbound: self.inbound.clone(),
            cancel: self.jobs[&job].clone(),
        });
        let _ = self.events.send(Event::ImportStarted { job, source_id });
        Ok(Outcome::ImportStarted { job })
    }

    /// Starts the background scan of a source (`Command::IndexSource`).
    fn start_index(&mut self, source_id: SourceId, merge: Vec<SourceId>) -> Result<Outcome> {
        let entry = self.source_entry(&source_id)?;
        let root = Self::source_root(&entry)?;
        let source = Self::open_source(&entry)?;
        let slash = |relative: PathBuf| {
            relative
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        };
        // Other sources whose folder is inside this one: their files belong to them, unless they
        // are being merged into this one, in which case they are this source's own.
        let mut skip = Vec::new();
        let mut merged = Vec::new();
        for other in self.read_sources()?.sources {
            if other.id == source_id {
                continue;
            }
            let Ok(other_root) = Self::source_root(&other) else {
                continue;
            };
            let Some(relative) = crate::paths::relative_to(&other_root, &root)
                .filter(|relative| !relative.as_os_str().is_empty())
            else {
                continue;
            };
            if merge.contains(&other.id) {
                merged.push((other.id, slash(relative)));
            } else {
                skip.push(slash(relative));
            }
        }
        let job = self.spawn_job();
        let (answer_tx, answer_rx) = mpsc::channel();
        self.index_decisions.insert(job, answer_tx);
        let catalogue_path = self
            .catalogue
            .path()
            .expect("the engine's catalogue is always a file")
            .to_path_buf();
        index_job::spawn(IndexJob {
            job,
            workspace: self.workspace.clone(),
            source,
            source_id,
            skip,
            merge: merged,
            catalogue_path,
            events: self.events.clone(),
            inbound: self.inbound.clone(),
            cancel: self.jobs[&job].clone(),
            decision: answer_rx,
        });
        Ok(Outcome::IndexStarted { job })
    }

    /// Starts taking a source out (`Command::RemoveSource`).
    fn start_remove(&mut self, source_id: SourceId) -> Result<Outcome> {
        self.source_entry(&source_id)?;
        let job = self.spawn_job();
        let catalogue_path = self
            .catalogue
            .path()
            .expect("the engine's catalogue is always a file")
            .to_path_buf();
        remove_job::spawn(RemoveJob {
            job,
            workspace: self.workspace.clone(),
            source_id,
            catalogue_path,
            events: self.events.clone(),
            inbound: self.inbound.clone(),
            cancel: self.jobs[&job].clone(),
        });
        Ok(Outcome::RemoveStarted { job })
    }

    /// The photos of a removed source are gone: take the source out of the workspace's list and
    /// the catalogue's.
    fn finish_remove_source(
        &mut self,
        job: JobId,
        source_id: SourceId,
        removed: usize,
        kept: usize,
    ) {
        if let Ok(mut sources) = self.read_sources() {
            sources.sources.retain(|entry| entry.id != source_id);
            sources.updated = Timestamp::now();
            if self.workspace.write_sources(&sources).is_ok() {
                let _ = self.catalogue.remove_source(&source_id);
                let _ = self.events.send(Event::SourceRemoved {
                    job,
                    source_id,
                    removed,
                    kept,
                });
            }
        }
    }

    fn handle_imported(
        &mut self,
        photo: Option<PhotoSidecar>,
        stat: Option<SidecarStat>,
        backfill: Option<(PhotoId, ContentHash)>,
    ) {
        if let Some((id, hash)) = backfill {
            let _ = self.catalogue.apply_hash(&id, &hash);
        }
        if let (Some(photo), Some(stat)) = (photo, stat) {
            let id = photo.photo_id;
            if self.catalogue.apply_new_photo(&photo, stat).is_ok() {
                let _ = self.events.send(Event::PhotoChanged(id));
            }
        }
    }
}

/// `id` and every keyword whose parent chain includes it.
fn descendants_of(vocabulary: &[KeywordEntry], id: KeywordId) -> Vec<KeywordId> {
    let by_id: HashMap<KeywordId, &KeywordEntry> = vocabulary.iter().map(|k| (k.id, k)).collect();
    let mut out = Vec::new();
    for entry in vocabulary {
        let mut current = Some(entry.id);
        let mut seen = std::collections::HashSet::new();
        while let Some(cur) = current {
            if cur == id {
                out.push(entry.id);
                break;
            }
            if !seen.insert(cur) {
                break;
            }
            current = by_id.get(&cur).and_then(|k| k.parent);
        }
    }
    out
}
