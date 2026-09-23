// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

use auroraw_format::sidecar::Flag;
use auroraw_import::Profile;
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
    /// Imports every file `source_id` has (D-030: no pre-selection) that is not already in the
    /// catalogue (design note 004 §6.3, item 4), copying it (and a companion JPEG, per the
    /// profile's pair rule, D-032) into `destination_source_id`'s folder, verified by whole-file
    /// hash (item 5), with the profile's metadata template applied (D-029). Listing the source,
    /// reading each file's metadata, planning destinations and copying all run on a background
    /// worker (`Event::JobProgress`, one `Event::PhotoChanged` per photo as it lands): the
    /// coordinator itself never blocks on a large card (spec §5.2, "does not stall the interface
    /// thread"). `state_path` is where this job's resumable progress is kept (an interrupted
    /// import, resubmitted with the same path, picks up where it stopped); `backup_roots` are
    /// resolved host paths, one per entry of `profile.backup_templates`, in order.
    Import {
        /// The source being imported from (a card or a folder).
        source_id: SourceId,
        /// The already-registered source files are copied into.
        destination_source_id: SourceId,
        /// Destination templates, pairing, and the metadata template.
        profile: Profile,
        /// A session name for the `{shoot}` template token.
        shoot: Option<String>,
        /// Extra verified-copy destinations, resolved to real paths.
        backup_roots: Vec<PathBuf>,
        /// Where this job's resumable state is kept.
        state_path: PathBuf,
    },
    /// Scans a registered source in the background and adds every file it holds that the
    /// catalogue does not know as a photo (RAW and JPEG of one shot as one photo, D-032), reading
    /// each file's metadata into its sidecar (D-074): nothing is copied (spec §5.1, "adding a
    /// folder in place"). Reports `Event::IndexPlanned` first; if some of the files match photos
    /// that were removed earlier (their sidecars wait in the workspace's `removed/`), the job
    /// pauses there until [`Command::ContinueIndex`] says whether to restore them.
    IndexSource {
        /// The source to scan.
        source_id: SourceId,
        /// Sources whose folders are inside this one, to be merged into it first: their photos are
        /// kept, with their ratings and versions, and become this source's (each location gets this
        /// source's identifier and the folder's place in it), then their entries go.
        merge: Vec<SourceId>,
    },
    /// Answers the pause of an index job (`Event::IndexPlanned` with something to restore).
    ContinueIndex {
        /// The index job.
        job_id: crate::job::JobId,
        /// Whether to restore the photos that were removed earlier (with their ratings, keywords
        /// and versions), rather than add their files as new photos.
        restore: bool,
    },
    /// Takes a source out of the catalogue, in the background. Every photo whose original is only
    /// in this source is removed from the catalogue and its sidecars are moved, recoverably, to the
    /// workspace's `removed/`; a photo that has another location keeps it. Originals are never
    /// touched.
    RemoveSource {
        /// The source to remove.
        source_id: SourceId,
    },
}
