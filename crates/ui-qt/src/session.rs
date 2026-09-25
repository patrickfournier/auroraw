// SPDX-License-Identifier: GPL-3.0-or-later
//! What the Qt objects share: the open workspace's engine, and the collector the image provider waits
//! on. (The provider runs on Qt's image loading threads, outside every QObject.)

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use auroraw_engine::{Engine, ThumbnailService};
use auroraw_types::PhotoId;

/// An open workspace. It is dropped as soon as another opens, which releases the workspace's folder.
pub struct Session {
    pub engine: Engine,
    /// This machine's folder for what the workspace remembers (the import form, resumable imports).
    pub data_dir: std::path::PathBuf,
    pub thumbs: Collector,
}

static CURRENT: Mutex<Option<Arc<Session>>> = Mutex::new(None);

pub fn set_current(session: Option<Arc<Session>>) {
    *CURRENT.lock().unwrap() = session;
}

/// Lets go of the current session if it is `session` (the one a launcher that is being destroyed
/// opened; a newer launcher's own session stays).
pub fn clear_if_current(session: &Weak<Session>) {
    let mut current = CURRENT.lock().unwrap();
    if current
        .as_ref()
        .is_some_and(|c| std::ptr::eq(Arc::as_ptr(c), session.as_ptr()))
    {
        *current = None;
    }
}

pub fn current() -> Option<Arc<Session>> {
    CURRENT.lock().unwrap().clone()
}

#[derive(Default)]
struct Done {
    ready: HashMap<PhotoId, Vec<u8>>,
    /// Photos no thumbnail can be made for: not asked for again until the photo changes.
    failed: HashSet<PhotoId>,
    /// The requests (tokens of the image provider's responses) waiting for a photo's thumbnail.
    waiting: HashMap<PhotoId, Vec<u64>>,
}

/// Turns the thumbnail service's poll-based delivery into an answer for each request: a thread polls
/// the service and hands every waiting request its thumbnail (or its failure) through `deliver`.
pub struct Collector {
    service: Arc<ThumbnailService>,
    done: Arc<Mutex<Done>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    deliver: Deliver,
}

/// Answers one request: its token, and the JPEG of the thumbnail or `None` when there is none.
pub type Deliver = Arc<dyn Fn(u64, Option<&[u8]>) + Send + Sync>;

impl Collector {
    pub fn new(service: ThumbnailService, deliver: Deliver) -> Self {
        let service = Arc::new(service);
        let done: Arc<Mutex<Done>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (service, done, stop, deliver) =
                (service.clone(), done.clone(), stop.clone(), deliver.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let arrived = service.poll();
                    let failed = service.poll_failed();
                    if !arrived.is_empty() || !failed.is_empty() {
                        let mut answers: Vec<(u64, Option<Vec<u8>>)> = Vec::new();
                        {
                            let mut state = done.lock().unwrap();
                            if state.ready.len() > 4096 {
                                state.ready.clear();
                            }
                            for (id, thumbnail) in arrived {
                                for token in state.waiting.remove(&id).unwrap_or_default() {
                                    answers.push((token, Some(thumbnail.jpeg.clone())));
                                }
                                state.ready.insert(id, thumbnail.jpeg);
                            }
                            for id in failed {
                                for token in state.waiting.remove(&id).unwrap_or_default() {
                                    answers.push((token, None));
                                }
                                state.failed.insert(id);
                            }
                        }
                        for (token, jpeg) in answers {
                            deliver(token, jpeg.as_deref());
                        }
                    }
                    std::thread::sleep(Duration::from_millis(4));
                }
            })
        };
        Self {
            service,
            done,
            stop,
            thread: Some(thread),
            deliver,
        }
    }

    /// Makes a photo's thumbnail ahead of anybody asking for it (a photo that just entered the
    /// catalogue), behind whatever the grid asks for. A photo that changed may have a thumbnail
    /// now, so an earlier failure is forgotten.
    pub fn warm(&self, id: PhotoId) {
        self.done.lock().unwrap().failed.remove(&id);
        self.service.warm(id);
    }

    /// Asks for a photo's thumbnail on behalf of the request `token`, answered through the collector's
    /// `deliver` (from this call, when the thumbnail is already known, or later from the collector's thread).
    pub fn request(&self, id: PhotoId, token: u64) {
        let known = {
            let mut state = self.done.lock().unwrap();
            if let Some(bytes) = state.ready.get(&id) {
                Some(Some(bytes.clone()))
            } else if state.failed.contains(&id) {
                Some(None)
            } else {
                state.waiting.entry(id).or_default().push(token);
                None
            }
        };
        match known {
            Some(jpeg) => (self.deliver)(token, jpeg.as_deref()),
            None => self.service.request(id),
        }
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
