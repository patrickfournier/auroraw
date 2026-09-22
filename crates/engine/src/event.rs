// SPDX-License-Identifier: GPL-3.0-or-later
use auroraw_types::{KeywordId, PhotoId};

use crate::command::Command;
use crate::job::JobId;

/// Something the engine reports, in the order it happened (architecture §4.3). Delivered on
/// [`crate::EventReceiver`]; batch-drain it rather than reacting to one at a time (§4.3: events
/// are batched, not one wake-up each).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A command was applied. Carries the command itself, so a caller that submitted several
    /// commands from different threads can reconstruct the exact order the single writer applied
    /// them in (testing strategy §8, the concurrency check).
    Applied(Command),
    /// A command failed; the engine is otherwise unaffected; the next command still runs
    /// (matching the plugin host's "never take the process down", extended here to commands).
    Failed {
        /// The command that failed.
        command: Command,
        /// Why, as text (the concrete error type does not need to cross the event channel).
        error: String,
    },
    /// A rebuild finished.
    RebuildFinished {
        /// How many photos the new catalogue has.
        photos: usize,
        /// How many versions.
        versions: usize,
    },
    /// A reconcile finished.
    ReconcileFinished {
        /// How many photo sidecars were re-read and reapplied.
        changed_photos: usize,
        /// How many photos the workspace no longer has.
        removed_photos: usize,
    },
    /// A photo's catalogue row (and, for a rating or flag change, its sidecar) changed.
    PhotoChanged(PhotoId),
    /// A keyword was added to the vocabulary.
    KeywordCreated(KeywordId),
    /// A keyword was renamed; the catalogue already reflects it. `affected` sidecars still carry
    /// the old name as a snapshot (note 003 §6) and are refreshed by the background job `job`.
    KeywordRenamed {
        /// The keyword.
        keyword_id: KeywordId,
        /// The background refresh job doing the rest of the work.
        job: JobId,
        /// How many sidecars it will touch.
        affected: usize,
    },
    /// A background job made progress.
    JobProgress {
        /// The job.
        job: JobId,
        /// How many items it has finished.
        done: usize,
        /// How many it has in total.
        total: usize,
    },
    /// A background job finished (ran to completion).
    JobFinished(JobId),
    /// A background job stopped early, cancelled by `Command::CancelJob`.
    JobCancelled(JobId),
    /// The engine has stopped: no more events follow.
    Stopped,
}
