// SPDX-License-Identifier: GPL-3.0-or-later
//! The image view's service (WP9, D-100): the photo on screen at a size worth looking at, and the
//! photos around it made ahead of time, so that a key press finds its image ready. Shaped like
//! [`crate::ThumbnailService`] (worker threads with their own connection to the catalogue, nothing
//! that touches the coordinator's), but the pictures are big and few: they are kept in a small
//! in-memory LRU of encoded JPEGs (8 by default) instead of a database.
//!
//! A person walking through a folder asks for the photo on screen ([`PreviewService::request`],
//! answered by [`PreviewService::poll`]) and says which photos are next ([`PreviewService::prefetch`],
//! which replaces the previous list: photos the view has left behind are no longer worth making).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use auroraw_catalogue::Catalogue;
use auroraw_imaging::{Thumbnail, VIEW_MAX_EDGE, view_image};
use auroraw_types::PhotoId;
use auroraw_workspace::Workspace;

use crate::error::Result;
use crate::thumbnails::source_root;

/// How many pictures are kept.
pub const DEFAULT_CAPACITY: usize = 8;

/// The pictures made, the most recently used last.
struct Cache {
    capacity: usize,
    order: VecDeque<PhotoId>,
    images: HashMap<PhotoId, Arc<Thumbnail>>,
}

impl Cache {
    fn get(&mut self, id: &PhotoId) -> Option<Arc<Thumbnail>> {
        let image = self.images.get(id)?.clone();
        self.order.retain(|other| other != id);
        self.order.push_back(*id);
        Some(image)
    }

    fn put(&mut self, id: PhotoId, image: Arc<Thumbnail>) {
        self.order.retain(|other| *other != id);
        self.order.push_back(id);
        self.images.insert(id, image);
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.images.remove(&oldest);
            }
        }
    }
}

/// What waits for a worker, and what somebody is waiting for.
#[derive(Default)]
struct Queues {
    /// Asked for by [`PreviewService::request`], the newest first.
    urgent: Vec<PhotoId>,
    /// Given by [`PreviewService::prefetch`], in the order they will be needed.
    warm: VecDeque<PhotoId>,
    /// A worker is making these.
    active: HashSet<PhotoId>,
    /// Asked for and not yet delivered (or reported failed).
    wanted: HashSet<PhotoId>,
}

#[derive(Default)]
struct Inbox {
    ready: Vec<(PhotoId, Arc<Thumbnail>)>,
    failed: Vec<PhotoId>,
}

struct Shared {
    queue: Mutex<Queues>,
    cv: Condvar,
    stop: AtomicBool,
    cache: Mutex<Cache>,
    inbox: Mutex<Inbox>,
}

/// A background service that turns a photo id into the picture the image view shows.
pub struct PreviewService {
    shared: Arc<Shared>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl PreviewService {
    /// Starts `workers` worker threads (two are enough: the photo on screen and the next one) keeping
    /// `capacity` pictures.
    pub fn start(
        workspace: Arc<Workspace>,
        catalogue_path: PathBuf,
        workers: usize,
        capacity: usize,
    ) -> Result<Self> {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queues::default()),
            cv: Condvar::new(),
            stop: AtomicBool::new(false),
            cache: Mutex::new(Cache {
                capacity: capacity.max(1),
                order: VecDeque::new(),
                images: HashMap::new(),
            }),
            inbox: Mutex::new(Inbox::default()),
        });
        let handles = (0..workers.max(1))
            .map(|_| {
                let shared = shared.clone();
                let workspace = workspace.clone();
                let catalogue_path = catalogue_path.clone();
                std::thread::spawn(move || worker(shared, workspace, catalogue_path))
            })
            .collect();
        Ok(Self {
            shared,
            workers: handles,
        })
    }

    /// Asks for `id`'s picture: it is delivered by [`Self::poll`] at once when it is already made, else when a
    /// worker has made it (before any photo that was only asked for ahead of time), and reported by
    /// [`Self::poll_failed`] when it cannot be made. Asking again before it arrived asks nothing more.
    pub fn request(&self, id: PhotoId) {
        let cached = self.shared.cache.lock().expect("not poisoned").get(&id);
        if let Some(image) = cached {
            self.shared
                .inbox
                .lock()
                .expect("not poisoned")
                .ready
                .push((id, image));
            return;
        }
        let mut queue = self.shared.queue.lock().expect("not poisoned");
        if !queue.wanted.insert(id) {
            return;
        }
        // A worker already making it (it was asked for ahead of time) delivers it when done.
        if queue.active.contains(&id) {
            return;
        }
        // One that finished between the look above and the lock is there now.
        let cached = self.shared.cache.lock().expect("not poisoned").get(&id);
        if let Some(image) = cached {
            queue.wanted.remove(&id);
            drop(queue);
            self.shared
                .inbox
                .lock()
                .expect("not poisoned")
                .ready
                .push((id, image));
            return;
        }
        queue.warm.retain(|other| *other != id);
        queue.urgent.push(id);
        self.shared.cv.notify_one();
    }

    /// Says which photos the view will want next, the most likely first; they are made when no worker has a
    /// photo asked for. The previous list is forgotten.
    pub fn prefetch(&self, ids: &[PhotoId]) {
        let made: HashSet<PhotoId> = {
            let cache = self.shared.cache.lock().expect("not poisoned");
            ids.iter()
                .filter(|id| cache.images.contains_key(id))
                .copied()
                .collect()
        };
        let mut queue = self.shared.queue.lock().expect("not poisoned");
        queue.warm = ids
            .iter()
            .filter(|id| {
                !made.contains(id) && !queue.active.contains(id) && !queue.urgent.contains(id)
            })
            .copied()
            .collect();
        drop(queue);
        self.shared.cv.notify_all();
    }

    /// Every picture a worker (or the cache) has given since the last call, without blocking.
    pub fn poll(&self) -> Vec<(PhotoId, Arc<Thumbnail>)> {
        std::mem::take(&mut self.shared.inbox.lock().expect("not poisoned").ready)
    }

    /// Every photo a worker gave up on since the last call (the original is not there, or unreadable).
    pub fn poll_failed(&self) -> Vec<PhotoId> {
        std::mem::take(&mut self.shared.inbox.lock().expect("not poisoned").failed)
    }

    /// Whether `id`'s picture is made and kept (a request would be answered at once).
    pub fn is_ready(&self, id: &PhotoId) -> bool {
        self.shared
            .cache
            .lock()
            .expect("not poisoned")
            .images
            .contains_key(id)
    }

    /// How many pictures are kept.
    pub fn kept(&self) -> usize {
        self.shared.cache.lock().expect("not poisoned").images.len()
    }
}

impl Drop for PreviewService {
    fn drop(&mut self) {
        // Under the queue's lock, as `ThumbnailService` does: a worker about to wait must not miss it.
        {
            let _queue = self.shared.queue.lock().expect("not poisoned");
            self.shared.stop.store(true, Ordering::Relaxed);
        }
        self.shared.cv.notify_all();
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

fn worker(shared: Arc<Shared>, workspace: Arc<Workspace>, catalogue_path: PathBuf) {
    let Ok(catalogue) = Catalogue::open(&catalogue_path) else {
        return;
    };
    loop {
        let id = {
            let mut queue = shared.queue.lock().expect("not poisoned");
            loop {
                if shared.stop.load(Ordering::Relaxed) {
                    return;
                }
                let next = queue.urgent.pop().or_else(|| queue.warm.pop_front());
                if let Some(id) = next {
                    queue.active.insert(id);
                    break id;
                }
                queue = shared.cv.wait(queue).expect("not poisoned");
            }
        };
        let made = generate(&catalogue, &workspace, id);
        if let Some(image) = &made {
            shared
                .cache
                .lock()
                .expect("not poisoned")
                .put(id, image.clone());
        }
        let wanted = {
            let mut queue = shared.queue.lock().expect("not poisoned");
            queue.active.remove(&id);
            queue.wanted.remove(&id)
        };
        if wanted {
            let mut inbox = shared.inbox.lock().expect("not poisoned");
            match made {
                Some(image) => inbox.ready.push((id, image)),
                None => inbox.failed.push(id),
            }
        }
    }
}

fn generate(catalogue: &Catalogue, workspace: &Workspace, id: PhotoId) -> Option<Arc<Thumbnail>> {
    let row = catalogue.photo(&id).ok().flatten()?;
    let root = source_root(workspace, row.source_id?)?;
    let full = root.join(row.path?.replace('/', std::path::MAIN_SEPARATOR_STR));
    let orientation = workspace
        .read_photo(&id)
        .ok()
        .flatten()
        .and_then(|loaded| loaded.current())
        .and_then(|photo| photo.meta.original.orientation);
    view_image(&full, orientation, VIEW_MAX_EDGE)
        .ok()
        .map(Arc::new)
}
