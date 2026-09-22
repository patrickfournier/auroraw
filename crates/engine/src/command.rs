// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

use auroraw_format::sidecar::Flag;
use auroraw_types::{KeywordId, PhotoId, SourceId};

/// A change the engine's single writer applies, in the order it receives them (architecture
/// §4.3). Sent with [`crate::Engine::submit`] (fire and forget) or
/// [`crate::Engine::submit_and_wait`] (blocks for the outcome).
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Rebuilds the catalogue from the workspace (architecture §5.4).
    Rebuild,
    /// Compares the workspace against the catalogue's stored stats and refreshes every sidecar
    /// that changed (architecture §5.3): this is how an edit made outside Auroraw, or by another
    /// machine, reaches the catalogue.
    Reconcile,
    /// Sets a photo's own rating (0 to 5). The effective rating (D-063) follows unless the main
    /// version overrides it.
    SetRating {
        /// The photo.
        photo_id: PhotoId,
        /// The new rating.
        rating: u8,
    },
    /// Sets a photo's own flag.
    SetFlag {
        /// The photo.
        photo_id: PhotoId,
        /// The new flag, or `None` to clear it.
        flag: Option<Flag>,
    },
    /// Adds a keyword to a photo.
    AddKeyword {
        /// The photo.
        photo_id: PhotoId,
        /// The keyword.
        keyword_id: KeywordId,
    },
    /// Removes a keyword from a photo.
    RemoveKeyword {
        /// The photo.
        photo_id: PhotoId,
        /// The keyword.
        keyword_id: KeywordId,
    },
    /// Adds a keyword to the vocabulary.
    CreateKeyword {
        /// Its name.
        name: String,
        /// Its parent, or `None` for a top-level keyword.
        parent: Option<KeywordId>,
    },
    /// Renames a keyword. Changes the vocabulary and every catalogue row it (or a descendant of
    /// it) affects immediately; the sidecars that carry a now-stale name snapshot are refreshed
    /// afterwards, in the background (note 003 §6), reported through
    /// [`crate::Event::KeywordRenamed`] and the `Job*` events that follow it.
    RenameKeyword {
        /// The keyword.
        keyword_id: KeywordId,
        /// Its new name.
        new_name: String,
    },
    /// Cancels a background job (a keyword rename's sidecar refresh) started earlier.
    CancelJob {
        /// The job to cancel.
        job_id: crate::job::JobId,
    },
    /// Registers a folder or a removable volume's mount point as a source, "in place": nothing
    /// is copied (spec §5.1). `root` is this machine's real path to it, kept only in the
    /// workspace's `hint` and the catalogue (design note 002 §6.6), never synced.
    AddSource {
        /// Its display name.
        name: String,
        /// Where it is on this machine.
        root: PathBuf,
        /// `sources::filesystem::LOCAL_FOLDER` or `sources::filesystem::REMOVABLE_VOLUME`.
        kind: String,
    },
    /// Scans a source and reconciles what it finds against the catalogue (design note 004
    /// §6.3-§6.4): confirms unchanged files, marks a changed or missing one, relinks a moved or
    /// renamed one silently, and reports new and ambiguous files for
    /// [`Command::AddNewPhotos`] or a person to resolve. Does nothing if the source is not
    /// reachable right now.
    ScanSource {
        /// The source to scan.
        source_id: SourceId,
    },
    /// Adds photos for files a [`Command::ScanSource`] reported as new, once confirmed (D-019):
    /// never on its own. Each photo's metadata starts empty; reading it is WP5's job.
    AddNewPhotos {
        /// The source the files were found in.
        source_id: SourceId,
        /// Their paths inside the source, as `Event::SourceScanned` (or a fresh scan) reported.
        paths: Vec<String>,
    },
}
