// SPDX-License-Identifier: GPL-3.0-or-later
//! What the Qt objects share: the open workspace's engine, and the collector the image provider
//! waits on. (The provider runs on Qt's image loading threads, outside every QObject.)

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use auroraw_engine::{Engine, EventReceiver, ThumbnailService};
use auroraw_types::PhotoId;

/// An open workspace.
pub struct Session {
    pub engine: Engine,
    pub thumbs: Collector,
    #[allow(dead_code)]
    pub events: Mutex<EventReceiver>,
}

static CURRENT: Mutex<Option<Arc<Session>>> = Mutex::new(None);

pub fn set_current(session: Option<Arc<Session>>) {
    *CURRENT.lock().unwrap() = session;
}

pub fn current() -> Option<Arc<Session>> {
    CURRENT.lock().unwrap().clone()
}

#[derive(Default)]
struct Done {
    ready: HashMap<PhotoId, Vec<u8>>,
    failed: HashSet<PhotoId>,
}

/// Turns the thumbnail service's poll-based delivery into something a blocking caller can wait on.
pub struct Collector {
    service: Arc<ThumbnailService>,
    done: Arc<(Mutex<Done>, Condvar)>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Collector {
    pub fn new(service: ThumbnailService) -> Self {
        let service = Arc::new(service);
        let done: Arc<(Mutex<Done>, Condvar)> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (service, done, stop) = (service.clone(), done.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let arrived = service.poll();
                    let failed = service.poll_failed();
                    if !arrived.is_empty() || !failed.is_empty() {
                        let mut state = done.0.lock().unwrap();
                        if state.ready.len() > 4096 {
                            state.ready.clear();
                        }
                        for (id, thumbnail) in arrived {
                            state.ready.insert(id, thumbnail.jpeg);
                        }
                        state.failed.extend(failed);
                        done.1.notify_all();
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
        }
    }

    /// The JPEG of a photo's thumbnail, made if need be; `None` when it cannot be made.
    pub fn jpeg(&self, id: PhotoId) -> Option<Vec<u8>> {
        let (lock, cv) = &*self.done;
        {
            let state = lock.lock().unwrap();
            if let Some(bytes) = state.ready.get(&id) {
                return Some(bytes.clone());
            }
        }
        self.service.request(id);
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut state = lock.lock().unwrap();
        loop {
            if let Some(bytes) = state.ready.get(&id) {
                return Some(bytes.clone());
            }
            if state.failed.contains(&id) || Instant::now() > deadline {
                return None;
            }
            state = cv.wait_timeout(state, Duration::from_millis(50)).unwrap().0;
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
