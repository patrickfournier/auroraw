// SPDX-License-Identifier: GPL-3.0-or-later
//! The application core: owns the workspace and the catalogue, is their only writer, and is the
//! only thing the interface talks to (architecture §3.2, §4). Runs without a window (`cli`
//! drives it directly; WP4 and later work packages add the sources, import and pipeline this
//! coordinates).
//!
//! [`Engine::create`] or [`Engine::open`] starts a coordinator thread that owns the workspace and
//! the catalogue's single writable connection (architecture §4.2), and hands back an [`Engine`]
//! handle (cheap to clone, safe to share across threads: [`Engine::submit`] and
//! [`Engine::submit_and_wait`] just send a [`Command`] down a channel) and an [`EventReceiver`]
//! that reports what happened, in order (architecture §4.3). Reads bypass the coordinator
//! entirely: [`Engine::read_catalogue`] opens a fresh connection (SQLite's WAL mode, many readers
//! and one writer, is exactly this).
//!
//! The coordinator thread stops when every [`Engine`] handle for it has been dropped.

mod command;
mod coordinator;
mod error;
mod event;
mod job;
mod refresh;

pub use command::Command;
pub use coordinator::Outcome;
pub use error::{EngineError, Result};
pub use event::Event;
pub use job::JobId;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use auroraw_catalogue::Catalogue;
use auroraw_workspace::Workspace;

use coordinator::{Coordinator, Inbound};

/// A handle to a running engine. Cheap to clone; every clone can submit commands from its own
/// thread (testing strategy §8: the single-writer coordinator is what serializes them, not this
/// handle).
#[derive(Clone)]
pub struct Engine {
    inbound: mpsc::Sender<Inbound>,
    workspace: Arc<Workspace>,
    catalogue_path: PathBuf,
}

/// The engine's event stream. Drain it rather than reacting one event at a time (architecture
/// §4.3: events are meant to be batched).
pub struct EventReceiver(mpsc::Receiver<Event>);

impl EventReceiver {
    /// Blocks for the next event, or `None` once the coordinator has stopped and every event is
    /// delivered.
    pub fn recv(&self) -> Option<Event> {
        self.0.recv().ok()
    }

    /// The next event if one is already waiting, without blocking.
    pub fn try_recv(&self) -> Option<Event> {
        self.0.try_recv().ok()
    }

    /// Blocks for up to `timeout` for the next event.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.0.recv_timeout(timeout).ok()
    }

    /// Every event waiting right now, in order, without blocking: the batch-drain architecture
    /// §4.3 describes.
    pub fn drain(&self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = self.0.try_recv() {
            events.push(event);
        }
        events
    }
}

impl Engine {
    /// The application version.
    pub fn version() -> &'static str {
        auroraw_types::app_version()
    }

    /// Creates a new workspace and catalogue and starts the engine over them.
    pub fn create(
        workspace_path: &Path,
        catalogue_path: &Path,
        catalogue_name: &str,
    ) -> Result<(Self, EventReceiver)> {
        let workspace = Workspace::create(workspace_path, catalogue_name)?;
        let catalogue = Catalogue::create(catalogue_path, workspace.workspace_id())?;
        Ok(Self::spawn(
            workspace,
            catalogue,
            catalogue_path.to_path_buf(),
        ))
    }

    /// Opens an existing workspace and catalogue. Refuses the pair if the catalogue does not
    /// index this workspace (note 001 §5.3): submit [`Command::Rebuild`] after opening the
    /// catalogue on its own (`Catalogue::open`) to recover from that, or rebuild through the CLI.
    pub fn open(workspace_path: &Path, catalogue_path: &Path) -> Result<(Self, EventReceiver)> {
        let workspace = Workspace::open(workspace_path)?;
        let catalogue = Catalogue::open(catalogue_path)?;
        if catalogue.workspace_id()? != workspace.workspace_id() {
            return Err(EngineError::WrongCatalogue);
        }
        Ok(Self::spawn(
            workspace,
            catalogue,
            catalogue_path.to_path_buf(),
        ))
    }

    fn spawn(
        workspace: Workspace,
        catalogue: Catalogue,
        catalogue_path: PathBuf,
    ) -> (Self, EventReceiver) {
        let workspace = Arc::new(workspace);
        let (inbound_tx, inbound_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let coordinator =
            Coordinator::new(workspace.clone(), catalogue, event_tx, inbound_tx.clone());
        std::thread::spawn(move || coordinator.run(inbound_rx));
        (
            Self {
                inbound: inbound_tx,
                workspace,
                catalogue_path,
            },
            EventReceiver(event_rx),
        )
    }

    /// Sends a command without waiting for it to apply. What happened arrives on the
    /// [`EventReceiver`], in order, as [`Event::Applied`] or [`Event::Failed`].
    pub fn submit(&self, command: Command) -> Result<()> {
        self.inbound
            .send(Inbound::Command {
                command,
                reply: None,
            })
            .map_err(|_| EngineError::Stopped)
    }

    /// Sends a command and blocks until the coordinator has applied it (or failed to), returning
    /// its outcome directly. Safe to call from several threads at once: each call blocks only its
    /// own caller, and the coordinator still applies commands one at a time, in the order it
    /// receives them from all callers combined.
    pub fn submit_and_wait(&self, command: Command) -> Result<Outcome> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.inbound
            .send(Inbound::Command {
                command,
                reply: Some(reply_tx),
            })
            .map_err(|_| EngineError::Stopped)?;
        reply_rx.recv().map_err(|_| EngineError::Stopped)?
    }

    /// The open workspace, for reading directly (its read methods take `&self` and need no
    /// coordination; only writes go through commands).
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// A fresh, independent connection to the catalogue, for queries (`list_recent`, `search`,
    /// ...). Safe alongside the coordinator's own writes: SQLite's WAL mode is many readers, one
    /// writer (architecture §4.2). Never write through the result of this: the coordinator is the
    /// catalogue's only writer, by convention this crate does not enforce at the type level.
    pub fn read_catalogue(&self) -> Result<Catalogue> {
        Ok(Catalogue::open(&self.catalogue_path)?)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use auroraw_format::sidecar::PhotoSidecar;
    use auroraw_testkit::temp_dir;
    use auroraw_types::{KeywordId, PhotoId};

    use super::*;
    use crate::command::Command;

    fn new_engine() -> (Engine, EventReceiver, auroraw_testkit::TempDir) {
        let dir = temp_dir();
        let (engine, events) = Engine::create(
            &dir.path().join("Main"),
            &dir.path().join("main.sqlite"),
            "Main",
        )
        .unwrap();
        (engine, events, dir)
    }

    fn wait_for(
        events: &EventReceiver,
        mut matches: impl FnMut(&Event) -> bool,
        timeout: Duration,
    ) -> Event {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let event = events
                .recv_timeout(remaining)
                .expect("event before timeout");
            if matches(&event) {
                return event;
            }
        }
    }

    #[test]
    fn version_is_not_empty() {
        assert!(!Engine::version().is_empty());
    }

    #[test]
    fn opening_a_catalogue_for_the_wrong_workspace_is_refused() {
        let dir = temp_dir();
        let (_engine, _events) =
            Engine::create(&dir.path().join("A"), &dir.path().join("a.sqlite"), "A").unwrap();
        let other = Workspace::create(&dir.path().join("B"), "B").unwrap();
        assert!(matches!(
            Engine::open(other.root(), &dir.path().join("a.sqlite")),
            Err(EngineError::WrongCatalogue)
        ));
    }

    #[test]
    fn set_rating_updates_the_sidecar_and_the_catalogue() {
        let (engine, events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();
        events.drain();

        engine
            .submit_and_wait(Command::SetRating {
                photo_id,
                rating: 4,
            })
            .unwrap();
        wait_for(
            &events,
            |e| matches!(e, Event::PhotoChanged(id) if *id == photo_id),
            Duration::from_secs(5),
        );

        let catalogue = engine.read_catalogue().unwrap();
        let row = catalogue.photo(&photo_id).unwrap().expect("indexed");
        assert_eq!(row.rating, 4);
        assert_eq!(row.effective_rating, 4);

        let sidecar = engine
            .workspace()
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        assert_eq!(sidecar.meta.rating, Some(4));
    }

    #[test]
    fn set_rating_on_an_unknown_photo_is_an_error() {
        let (engine, _events, _dir) = new_engine();
        let err = engine
            .submit_and_wait(Command::SetRating {
                photo_id: PhotoId::random(),
                rating: 3,
            })
            .unwrap_err();
        assert!(matches!(err, EngineError::NotFound { kind: "photo", .. }));
    }

    #[test]
    fn add_and_remove_keyword_round_trips_through_the_catalogue() {
        let (engine, events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();
        events.drain();

        let Outcome::KeywordCreated(keyword_id) = engine
            .submit_and_wait(Command::CreateKeyword {
                name: "Heron".into(),
                parent: None,
            })
            .unwrap()
        else {
            panic!("expected KeywordCreated");
        };

        engine
            .submit_and_wait(Command::AddKeyword {
                photo_id,
                keyword_id,
            })
            .unwrap();
        let sidecar = engine
            .workspace()
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        assert_eq!(sidecar.meta.keyword_ids, vec![keyword_id]);
        assert_eq!(sidecar.meta.keyword_paths, vec!["Heron".to_string()]);

        let catalogue = engine.read_catalogue().unwrap();
        assert_eq!(
            catalogue.photos_with_keywords(&[keyword_id]).unwrap(),
            vec![photo_id]
        );

        engine
            .submit_and_wait(Command::RemoveKeyword {
                photo_id,
                keyword_id,
            })
            .unwrap();
        let sidecar = engine
            .workspace()
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        assert!(sidecar.meta.keyword_ids.is_empty());
        let catalogue = engine.read_catalogue().unwrap();
        assert!(
            catalogue
                .photos_with_keywords(&[keyword_id])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn adding_an_unknown_keyword_is_an_error() {
        let (engine, _events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();

        let err = engine
            .submit_and_wait(Command::AddKeyword {
                photo_id,
                keyword_id: KeywordId::random(),
            })
            .unwrap_err();
        assert!(matches!(
            err,
            EngineError::NotFound {
                kind: "keyword",
                ..
            }
        ));
    }

    #[test]
    fn renaming_a_keyword_refreshes_every_sidecar_that_carries_it() {
        let (engine, events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();
        events.drain();

        let Outcome::KeywordCreated(keyword_id) = engine
            .submit_and_wait(Command::CreateKeyword {
                name: "Heron".into(),
                parent: None,
            })
            .unwrap()
        else {
            panic!("expected KeywordCreated");
        };
        engine
            .submit_and_wait(Command::AddKeyword {
                photo_id,
                keyword_id,
            })
            .unwrap();
        events.drain();

        let outcome = engine
            .submit_and_wait(Command::RenameKeyword {
                keyword_id,
                new_name: "Great Blue Heron".into(),
            })
            .unwrap();
        let Outcome::RenameStarted { job, affected } = outcome else {
            panic!("expected RenameStarted")
        };
        assert_eq!(affected, 1);

        wait_for(
            &events,
            |e| matches!(e, Event::JobFinished(id) if *id == job),
            Duration::from_secs(5),
        );

        let sidecar = engine
            .workspace()
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        assert_eq!(
            sidecar.meta.keyword_paths,
            vec!["Great Blue Heron".to_string()]
        );

        let catalogue = engine.read_catalogue().unwrap();
        let row = catalogue.photo(&photo_id).unwrap().unwrap();
        assert!(row.title.is_none()); // sanity: rename did not disturb unrelated fields
    }

    #[test]
    fn reconcile_picks_up_a_sidecar_edited_outside_the_engine() {
        let (engine, events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();
        events.drain();

        let mut sidecar = engine
            .workspace()
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        sidecar.meta.rating = Some(5);
        engine.workspace().write_photo(&sidecar).unwrap();

        engine.submit_and_wait(Command::Reconcile).unwrap();
        wait_for(
            &events,
            |e| matches!(e, Event::ReconcileFinished { .. }),
            Duration::from_secs(5),
        );

        let catalogue = engine.read_catalogue().unwrap();
        let row = catalogue.photo(&photo_id).unwrap().unwrap();
        assert_eq!(row.rating, 5);
    }

    #[test]
    fn commands_from_several_threads_are_applied_in_a_single_recorded_order() {
        let (engine, events, _dir) = new_engine();
        let photo_id = PhotoId::random();
        engine
            .workspace()
            .write_photo(&PhotoSidecar::new(photo_id))
            .unwrap();
        engine.submit_and_wait(Command::Rebuild).unwrap();
        events.drain();

        let threads: Vec<_> = (0..4u8)
            .map(|i| {
                let engine = engine.clone();
                std::thread::spawn(move || {
                    engine
                        .submit_and_wait(Command::SetRating {
                            photo_id,
                            rating: i % 6,
                        })
                        .unwrap();
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }

        let mut applied = Vec::new();
        for event in events.drain() {
            if let Event::Applied(Command::SetRating { rating, .. }) = event {
                applied.push(rating);
            }
        }
        assert_eq!(applied.len(), 4);

        // The coordinator applies commands one at a time (architecture §4.2): whichever
        // `SetRating` it recorded last is necessarily the one whose value survives.
        let catalogue = engine.read_catalogue().unwrap();
        let concurrent_rating = catalogue.photo(&photo_id).unwrap().unwrap().rating;
        assert_eq!(concurrent_rating, *applied.last().unwrap());
    }
}
