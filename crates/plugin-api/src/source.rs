// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `Source` family's interface (architecture §8.1): list, stat, read a range, watch, and
//! whether the source accepts writes. A source never writes on its own (D-018); nothing here
//! offers a write method. The engine reconciles what a source reports against the catalogue
//! (design note 004 §6.3-§6.4); this crate only describes what a source can be asked.

use std::fmt;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

/// Whether a source can currently be read (spec §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    /// Reachable.
    Online,
    /// Not reachable right now (an unplugged drive, a network share that is down), but the
    /// source's expected location still exists.
    Offline,
    /// The expected location no longer exists at all.
    Missing,
}

/// One file a source reports, from [`Source::list`] or [`Source::stat`].
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The path inside the source, relative to its root, with `/` separators (never absolute,
    /// never `..`).
    pub path: String,
    /// The size in bytes.
    pub size: u64,
    /// The modification time, when the source can report one.
    pub modified: Option<SystemTime>,
}

/// What went wrong asking a source something.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    /// The source is offline or missing; the operation needs it online.
    #[error("the source is not reachable")]
    Unreachable,
    /// No file at that path.
    #[error("no file at {path:?}")]
    NotFound {
        /// The path asked for.
        path: String,
    },
    /// The source refused for some other reason (permissions, a malformed path).
    #[error("{0}")]
    Other(String),
}

/// Stops watching when dropped.
pub struct Watch(pub Box<dyn std::any::Any + Send>);

impl fmt::Debug for Watch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Watch")
    }
}

/// A place files can be listed, read and (for a source that supports it) watched: a local
/// folder, a removable volume, and later a card reader, a phone or online storage (D-056), all
/// through the same interface. Never asked to write (D-018): copying at import and exporting XMP
/// are the only exceptions, and neither goes through this trait.
pub trait Source: Send + Sync {
    /// Whether the source can be read from right now.
    fn state(&self) -> SourceState;

    /// Whether this source ever accepts writes (a read-only card, an archive: `false`). Auroraw
    /// writes into a source only at import and on explicit XMP export (D-018); nothing else here
    /// checks this, callers that would write must.
    fn writable(&self) -> bool;

    /// Every file the source currently has, recursively. Fails with
    /// [`SourceError::Unreachable`] if the source is not [`SourceState::Online`].
    fn list(&self) -> Result<Vec<Entry>, SourceError>;

    /// One file's current size and modification time.
    fn stat(&self, path: &str) -> Result<Entry, SourceError>;

    /// Reads `len` bytes starting at `offset` in the named file (design note 004's fingerprint
    /// samples do not need the whole file).
    fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, SourceError>;

    /// Starts watching for changes, if this source can push them (a local folder, with `notify`;
    /// architecture §9.1). `on_change` receives one `()` whenever something might have changed;
    /// it is a signal to rescan, not a diff (matching how ReadDirectoryChangesW, inotify and
    /// FSEvents disagree on exactly what changed but agree that *something* did). A source that
    /// can only be rescanned on demand (a network share) returns `Ok(None)`: the caller schedules
    /// its own periodic rescan instead.
    fn watch(&self, on_change: Sender<()>) -> Result<Option<Watch>, SourceError>;
}
