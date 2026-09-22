// SPDX-License-Identifier: GPL-3.0-or-later
//! Background jobs: work that must not block the coordinator's command queue (architecture
//! §4.2's worker pool), with progress and cancellation (§4.3). The only job kind so far is a
//! keyword rename's sidecar refresh (note 003 §6); the shape is generic so a second kind (WP5's
//! thumbnail generation, for instance) has somewhere to plug in without redesigning this.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A background job's identifier, unique for the life of one engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(pub(crate) u64);

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "job-{}", self.0)
    }
}

/// A cooperative cancellation flag, cheap to clone and share between the job and whoever might
/// cancel it. The job checks it between units of work; nothing forces a job to stop mid-item
/// (matching `Read`'s own "finish what you started" convention elsewhere in this project).
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
