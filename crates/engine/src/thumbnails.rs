// SPDX-License-Identifier: GPL-3.0-or-later
//! A background thumbnail service (architecture §5.7, D-075; WP8): the grid asks for a photo's
//! preview by identifier and gets it immediately if it is already in the previews database, or a
//! signal once a worker thread has decoded, cached and delivered it. Mirrors spike 3's own
//! four-worker-thread shape, but generates a thumbnail on first request (spike 3's corpus was
//! pre-built) rather than only reading one that already exists.
//!
//! Worker threads own their own read connection to the catalogue and their own connection to the
//! previews database (SQLite's WAL, many readers and writers at once, D-073, matching
//! `catalogue::open`'s own reasoning); nothing here touches the coordinator's connections, so a
//! slow decode never competes with a command being applied.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use auroraw_catalogue::Catalogue;
use auroraw_imaging::{PreviewsDb, Thumbnail};
use auroraw_types::{PhotoId, SourceId};
use auroraw_workspace::Workspace;

use crate::error::Result;

/// What workers have finished and nobody has collected yet.
#[derive(Default)]
struct Inbox {
    ready: Vec<(PhotoId, Thumbnail)>,
    failed: Vec<PhotoId>,
}

/// What is waiting for a worker: thumbnails somebody is looking at, then thumbnails made ahead of
/// time.
#[derive(Default)]
struct Queues {
    /// Asked for by [`ThumbnailService::request`]: the newest first, since what a person scrolled
    /// to last is what is on screen.
    urgent: Vec<PhotoId>,
    /// Asked for by [`ThumbnailService::warm`]: in the order they were asked for, only when
    /// nothing urgent waits.
    warm: VecDeque<PhotoId>,
}

struct Shared {
    queue: Mutex<Queues>,
    cv: Condvar,
    stop: AtomicBool,
}

/// A background service that turns a photo id into a thumbnail, generating and caching one on
/// first request if none exists yet.
pub struct ThumbnailService {
    shared: Arc<Shared>,
    inbox: Arc<Mutex<Inbox>>,
    requested: Mutex<HashSet<PhotoId>>,
    warmed: Mutex<HashSet<PhotoId>>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl ThumbnailService {
    /// Starts `workers` worker threads (spike 3 measured four as enough to keep a scrolling grid
    /// fed) against the previews database at `previews_path` (created if it does not exist yet;
    /// this crate resolves no cache directory itself, matching `catalogue::Registry`'s own
    /// precedent -- the caller decides where it lives).
    pub fn start(
        workspace: Arc<Workspace>,
        catalogue_path: PathBuf,
        previews_path: PathBuf,
        workers: usize,
    ) -> Result<Self> {
        // Opened once, up front: fails fast on a bad path, and guarantees the schema exists
        // before any worker races to create it.
        PreviewsDb::open(&previews_path)?;
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queues::default()),
            cv: Condvar::new(),
            stop: AtomicBool::new(false),
        });
        let inbox = Arc::new(Mutex::new(Inbox::default()));
        let handles = (0..workers.max(1))
            .map(|_| {
                let shared = shared.clone();
                let inbox = inbox.clone();
                let workspace = workspace.clone();
                let catalogue_path = catalogue_path.clone();
                let previews_path = previews_path.clone();
                std::thread::spawn(move || {
                    worker(shared, inbox, workspace, catalogue_path, previews_path)
                })
            })
            .collect();
        Ok(Self {
            shared,
            inbox,
            requested: Mutex::new(HashSet::new()),
            warmed: Mutex::new(HashSet::new()),
            workers: handles,
        })
    }

    /// Asks for `id`'s thumbnail, if nothing has already asked for it and not yet received it.
    pub fn request(&self, id: PhotoId) {
        let mut requested = self.requested.lock().expect("not poisoned");
        if !requested.insert(id) {
            return;
        }
        self.shared
            .queue
            .lock()
            .expect("not poisoned")
            .urgent
            .push(id);
        self.shared.cv.notify_one();
    }

    /// Makes `id`'s thumbnail ahead of anybody asking for it, so that it is in the previews
    /// database when a grid shows the photo: only when no worker has a requested thumbnail to
    /// make, and in the order photos are given here (a scan gives them by name). Nothing is
    /// delivered by [`Self::poll`] for it; a photo given more than once is made once.
    pub fn warm(&self, id: PhotoId) {
        if !self.warmed.lock().expect("not poisoned").insert(id) {
            return;
        }
        self.shared
            .queue
            .lock()
            .expect("not poisoned")
            .warm
            .push_back(id);
        self.shared.cv.notify_one();
    }

    /// Every thumbnail a worker has finished since the last call, without blocking.
    pub fn poll(&self) -> Vec<(PhotoId, Thumbnail)> {
        let items = std::mem::take(&mut self.inbox.lock().expect("not poisoned").ready);
        let mut requested = self.requested.lock().expect("not poisoned");
        for (id, _) in &items {
            requested.remove(id);
        }
        items
    }

    /// Every photo a worker gave up on since the last call (the file is unreadable, or it has no
    /// embedded preview to make a thumbnail from), without blocking. Such a photo is asked for
    /// again only if [`Self::request`] is called for it after this returned it: a caller that
    /// wants to stop asking remembers it, or the grid would retry it on every redraw.
    pub fn poll_failed(&self) -> Vec<PhotoId> {
        let failed = std::mem::take(&mut self.inbox.lock().expect("not poisoned").failed);
        let mut requested = self.requested.lock().expect("not poisoned");
        for id in &failed {
            requested.remove(id);
        }
        failed
    }
}

impl Drop for ThumbnailService {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.cv.notify_all();
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

/// A source's root path on this machine, from the workspace's `sources.json` hint (design note
/// 002 §6.6): the same lookup `engine::import_job` does, read directly since a worker thread
/// needs no coordination for a read.
fn source_root(workspace: &Workspace, source_id: SourceId) -> Option<PathBuf> {
    let sources = workspace.read_sources().ok()??.current()?;
    let entry = sources.sources.into_iter().find(|s| s.id == source_id)?;
    entry
        .hint
        .get("path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
}

fn worker(
    shared: Arc<Shared>,
    inbox: Arc<Mutex<Inbox>>,
    workspace: Arc<Workspace>,
    catalogue_path: PathBuf,
    previews_path: PathBuf,
) {
    let (Ok(catalogue), Ok(previews)) = (
        Catalogue::open(&catalogue_path),
        PreviewsDb::open(&previews_path),
    ) else {
        return;
    };
    loop {
        let (id, wanted) = {
            let mut queue = shared.queue.lock().expect("not poisoned");
            loop {
                if shared.stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(id) = queue.urgent.pop() {
                    break (id, true);
                }
                if let Some(id) = queue.warm.pop_front() {
                    break (id, false);
                }
                queue = shared.cv.wait(queue).expect("not poisoned");
            }
        };
        let generated = generate(&catalogue, &previews, &workspace, id);
        if !wanted {
            // Made ahead of time: it is in the database now, or it will be reported when asked for.
            continue;
        }
        let mut inbox = inbox.lock().expect("not poisoned");
        match generated {
            Some(thumbnail) => inbox.ready.push((id, thumbnail)),
            None => inbox.failed.push(id),
        }
    }
}

fn generate(
    catalogue: &Catalogue,
    previews: &PreviewsDb,
    workspace: &Workspace,
    id: PhotoId,
) -> Option<Thumbnail> {
    if let Ok(Some(thumbnail)) = previews.get(&id) {
        return Some(thumbnail);
    }
    let row = catalogue.photo(&id).ok().flatten()?;
    let root = source_root(workspace, row.source_id?)?;
    let full = root.join(row.path?.replace('/', std::path::MAIN_SEPARATOR_STR));
    let (_, thumbnail, _) = auroraw_imaging::process(&full).ok()?;
    let _ = previews.put(&id, &thumbnail);
    Some(thumbnail)
}
