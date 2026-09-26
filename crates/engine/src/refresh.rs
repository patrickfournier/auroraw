// SPDX-License-Identifier: GPL-3.0-or-later
//! The background job that follows a keyword rename (note 003 §6): every sidecar that carries
//! the renamed keyword (or one of its descendants) gets its name snapshot refreshed. Runs on its
//! own thread, reading and writing sidecars directly through the shared `Workspace` (safe: each
//! call touches one file, and `Workspace`'s write is a self-contained atomic rename, note 001
//! §5.4); the matching catalogue row is updated back on the coordinator thread, the only place
//! that owns the database connection.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc;

use auroraw_types::{KeywordId, PhotoId};
use auroraw_workspace::Workspace;

use crate::coordinator::Inbound;
use crate::event::Event;
use crate::job::{CancelToken, JobId};

/// Refreshes the path snapshot of `photo_ids`' sidecars from `paths`. Each photo is read, changed and
/// written under the workspace's sidecar guard (a photo that another job moved away is not there any more
/// under it, and is skipped instead of being written back); the catalogue's row follows on the coordinator.
pub(crate) fn spawn(
    job: JobId,
    workspace: Arc<Workspace>,
    photo_ids: Vec<PhotoId>,
    paths: HashMap<KeywordId, String>,
    events: mpsc::Sender<Event>,
    inbound: mpsc::Sender<Inbound>,
    cancel: CancelToken,
) {
    std::thread::spawn(move || {
        let total = photo_ids.len();
        for (done, id) in photo_ids.into_iter().enumerate() {
            let done = done + 1;
            if cancel.is_cancelled() {
                let _ = events.send(Event::JobCancelled(job));
                let _ = inbound.send(Inbound::RefreshDone);
                return;
            }
            {
                let _guard = workspace.sidecar_guard();
                if let Some(loaded) = workspace.read_photo(&id).ok().flatten()
                    && let Some(mut photo) = loaded.current()
                {
                    for (path, keyword_id) in photo
                        .meta
                        .keyword_paths
                        .iter_mut()
                        .zip(photo.meta.keyword_ids.iter())
                    {
                        if let Some(fresh) = paths.get(keyword_id) {
                            *path = fresh.clone();
                        }
                    }
                    if workspace.write_photo(&photo).is_ok() {
                        let _ = inbound.send(Inbound::Refreshed { photo_id: id });
                    }
                }
            }
            let _ = events.send(Event::JobProgress { job, done, total });
        }
        let _ = events.send(Event::JobFinished(job));
        let _ = inbound.send(Inbound::RefreshDone);
    });
}
