// SPDX-License-Identifier: GPL-3.0-or-later
//! What can go wrong planning or running an import.

use std::path::PathBuf;

/// What went wrong.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// Reading from the source failed.
    #[error("cannot read {path:?} from the source: {source}")]
    Source {
        /// The path inside the source.
        path: String,
        /// Why.
        #[source]
        source: auroraw_plugin_api::source::SourceError,
    },
    /// A local file operation (writing a copy, reading one back) failed.
    #[error("{path}: {source}")]
    Io {
        /// The host path.
        path: PathBuf,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// A copy's destination did not read back with the same whole-file hash as the source
    /// (design note 004 §6.3, item 5): the copy is not trusted, and is not left in place.
    #[error("{path}: the copy does not match the source after being written back")]
    VerificationFailed {
        /// The destination that failed to verify.
        path: PathBuf,
    },
    /// The destination path a template rendered already exists and is not this same import
    /// resuming (its size does not match what is being copied): copying over it would either
    /// silently replace a file that is not ours or corrupt data mid-write (D-018 applies to every
    /// original, including ones already inside the archive).
    #[error("{path:?} already exists and does not look like the same file")]
    DestinationExists {
        /// The path that already exists.
        path: PathBuf,
    },
    /// The state of a resumable import could not be read as what it should be.
    #[error("cannot read the import's saved state at {path:?}: {source}")]
    State {
        /// Where it was being read from.
        path: PathBuf,
        /// Why.
        #[source]
        source: serde_json::Error,
    },
    /// A profile could not be read or written.
    #[error("{path:?}: {source}")]
    Profile {
        /// The file.
        path: PathBuf,
        /// Why.
        #[source]
        source: serde_json::Error,
    },
}

impl ImportError {
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

/// This crate's result type.
pub type Result<T> = std::result::Result<T, ImportError>;
