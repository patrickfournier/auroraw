// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

/// What can go wrong reading a photo's metadata, embedded preview or thumbnail. Never a panic:
/// a truncated or corrupt file is exactly the case this crate is asked to survive (testing
/// strategy §3, §8).
#[derive(Debug, thiserror::Error)]
pub enum ImagingError {
    /// The extension names a format this crate does not decode.
    #[error("{path:?}: unsupported format")]
    Unsupported {
        /// The file.
        path: PathBuf,
    },
    /// The file could not be opened or read.
    #[error("{path:?}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The decoder rejected the file: truncated, corrupt, or a variant it does not understand.
    #[error("{path:?}: {message}")]
    Decode {
        /// The file.
        path: PathBuf,
        /// What the decoder said.
        message: String,
    },
    /// Every format has some way to make a thumbnail from (an embedded preview, or the file
    /// itself); this file offered none. Not necessarily an error the caller should surface loudly
    /// -- a RAW with no embedded preview and no image engine yet (M2) to fall back to.
    #[error("{path:?}: no preview available")]
    NoPreview {
        /// The file.
        path: PathBuf,
    },
    /// The previews database refused an operation.
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
}

/// The result type this crate uses throughout.
pub type Result<T> = std::result::Result<T, ImagingError>;
