// SPDX-License-Identifier: GPL-3.0-or-later
//! The small, local, per-machine list of a person's catalogues (design note 001 §5.7): a
//! convenience, not a source of truth. If it is lost, it is rebuilt by opening each workspace
//! folder again. This module resolves no platform directory itself (that is `app`'s job, later);
//! it operates on whatever path its caller gives it.

use std::fs;
use std::path::{Path, PathBuf};

use auroraw_types::WorkspaceId;
use serde::{Deserialize, Serialize};

use crate::error::{CatalogueError, Result};

/// One entry of the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// The workspace's identifier.
    pub workspace_id: WorkspaceId,
    /// The catalogue's name, for display; not authoritative (the workspace marker is).
    pub name: String,
    /// Where the workspace was last found on this machine.
    pub workspace_path: PathBuf,
    /// Where this catalogue's database file lives.
    pub catalogue_path: PathBuf,
}

/// The list of catalogues this machine knows about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    entries: Vec<RegistryEntry>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads the registry from `path`; an empty registry if there is none yet.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|source| CatalogueError::Registry {
                    path: path.to_path_buf(),
                    source,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(CatalogueError::io(path, e)),
        }
    }

    /// Writes the registry to `path`, through a temporary file so a crash mid-write leaves the
    /// old registry (or nothing) rather than a truncated one.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| CatalogueError::io(parent, e))?;
        }
        let bytes = serde_json::to_vec_pretty(self).expect("a registry always serialises");
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &bytes).map_err(|e| CatalogueError::io(&tmp, e))?;
        fs::rename(&tmp, path).map_err(|e| CatalogueError::io(path, e))
    }

    /// Every entry, in no particular order.
    pub fn entries(&self) -> &[RegistryEntry] {
        &self.entries
    }

    /// The entry for a workspace, if it is registered.
    pub fn find(&self, workspace_id: WorkspaceId) -> Option<&RegistryEntry> {
        self.entries.iter().find(|e| e.workspace_id == workspace_id)
    }

    /// Adds a catalogue, or replaces the entry already registered for its workspace.
    pub fn upsert(&mut self, entry: RegistryEntry) {
        match self
            .entries
            .iter_mut()
            .find(|e| e.workspace_id == entry.workspace_id)
        {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
    }

    /// Removes a catalogue from the list (it is not deleted from disk).
    pub fn remove(&mut self, workspace_id: WorkspaceId) {
        self.entries.retain(|e| e.workspace_id != workspace_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> RegistryEntry {
        RegistryEntry {
            workspace_id: WorkspaceId::random(),
            name: name.to_string(),
            workspace_path: PathBuf::from("/photos/Main"),
            catalogue_path: PathBuf::from("/data/catalogue.db"),
        }
    }

    #[test]
    fn a_missing_registry_is_empty_not_an_error() {
        let dir = auroraw_testkit::temp_dir();
        let registry = Registry::load(&dir.path().join("registry.json")).unwrap();
        assert!(registry.entries().is_empty());
    }

    #[test]
    fn saving_and_loading_round_trips() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("registry.json");
        let mut registry = Registry::new();
        let a = entry("Main");
        let b = entry("Family");
        registry.upsert(a.clone());
        registry.upsert(b.clone());
        registry.save(&path).unwrap();

        let loaded = Registry::load(&path).unwrap();
        assert_eq!(loaded.entries().len(), 2);
        assert_eq!(loaded.find(a.workspace_id), Some(&a));
        assert_eq!(loaded.find(b.workspace_id), Some(&b));
        assert!(loaded.find(WorkspaceId::random()).is_none());
    }

    #[test]
    fn upsert_replaces_the_entry_for_the_same_workspace() {
        let mut registry = Registry::new();
        let mut a = entry("Main");
        registry.upsert(a.clone());
        a.name = "Renamed".into();
        registry.upsert(a.clone());
        assert_eq!(registry.entries().len(), 1);
        assert_eq!(registry.find(a.workspace_id).unwrap().name, "Renamed");
    }

    #[test]
    fn remove_takes_a_catalogue_out_of_the_list() {
        let mut registry = Registry::new();
        let a = entry("Main");
        registry.upsert(a.clone());
        registry.remove(a.workspace_id);
        assert!(registry.entries().is_empty());
    }
}
