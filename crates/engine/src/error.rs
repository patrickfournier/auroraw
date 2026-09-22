// SPDX-License-Identifier: GPL-3.0-or-later
use auroraw_catalogue::CatalogueError;
use auroraw_workspace::WorkspaceError;

/// What can go wrong asking the engine to do something.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The workspace refused the operation.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    /// The catalogue refused the operation.
    #[error(transparent)]
    Catalogue(#[from] CatalogueError),
    /// The catalogue does not index this workspace (note 001 §5.3): open with `Engine::rebuild`
    /// instead of `Engine::open`, or point at the right pair of paths.
    #[error("this catalogue does not belong to this workspace; rebuild it")]
    WrongCatalogue,
    /// The named entity is not in the workspace.
    #[error("{kind} {id} is not in the workspace")]
    NotFound {
        /// What kind of entity (`"photo"`, `"keyword"`).
        kind: &'static str,
        /// Its identifier, as text.
        id: String,
    },
    /// The coordinator thread has already stopped (a command was sent after `shutdown`, or it
    /// panicked: the latter is always a bug, since every command is caught, never propagated).
    #[error("the engine is no longer running")]
    Stopped,
    /// Reading a sidecar's size and modification time off disk failed (the file vanished between
    /// the workspace read and the stat, for instance).
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The result type this crate uses throughout.
pub type Result<T> = std::result::Result<T, EngineError>;
