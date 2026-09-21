// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

use auroraw_format::sidecar::SidecarError;
use auroraw_format::state::StateError;

/// What can go wrong with a workspace.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    /// A file system operation failed.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder concerned.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// The folder has no workspace marker.
    #[error("{0} is not an Auroraw workspace (no workspace.json)")]
    NotAWorkspace(PathBuf),
    /// The folder is already a workspace.
    #[error("{0} is already an Auroraw workspace")]
    AlreadyExists(PathBuf),
    /// The workspace was made by a newer version with another layout.
    #[error(
        "this workspace uses layout {found}, newer than the layout {supported} this version reads"
    )]
    NewerLayout {
        /// The layout found.
        found: u32,
        /// The layout supported.
        supported: u32,
    },
    /// The marker was written with a newer schema.
    #[error("the workspace marker was written by a newer version")]
    NewerMarker,
    /// Another program holds the writer's lock; this workspace is open read-only.
    #[error("the workspace is open read-only")]
    ReadOnly,
    /// A path is outside the workspace.
    #[error("{0} is not inside the workspace")]
    Outside(PathBuf),
    /// A file is larger than a sidecar or a state file can reasonably be.
    #[error("{path} is too large ({size} bytes)")]
    TooLarge {
        /// The file.
        path: PathBuf,
        /// Its size.
        size: u64,
    },
    /// A state file could not be read.
    #[error("{path}: {source}")]
    State {
        /// The file.
        path: PathBuf,
        /// The error.
        source: StateError,
    },
    /// A sidecar could not be read.
    #[error("{path}: {source}")]
    Sidecar {
        /// The file.
        path: PathBuf,
        /// The error.
        source: SidecarError,
    },
}

impl WorkspaceError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
