// SPDX-License-Identifier: GPL-3.0-or-later
//! Resumable per-job progress (spec §5.2: "an interrupted import resumes where it stopped"). Not
//! a workspace state file (design note 002): this is ephemeral, local, per-machine bookkeeping
//! for one import run, the way a download manager's own `.part` tracking is, not something a
//! rebuild needs or a sync tool should see. The caller decides where it lives; this crate does
//! not resolve a path for it (matching `catalogue::Registry`'s own precedent).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{ImportError, Result};

/// What became of one file this import job already looked at, keyed by its path inside the
/// source (unique within one job, since a source never lists the same path twice).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum ItemOutcome {
    /// Copied and verified.
    Copied,
    /// Not copied: already imported (design note 004 §6.3, item 4).
    Skipped {
        /// Why, for the report.
        reason: String,
    },
    /// Copying or verifying it failed; retried on the next resume.
    Failed {
        /// Why, for the report.
        error: String,
    },
}

/// One import job's progress: what happened to each file it has already looked at. A file this
/// state has no entry for yet has not been attempted; [`ItemOutcome::Failed`] is retried on
/// resume, [`ItemOutcome::Copied`] and [`ItemOutcome::Skipped`] are not attempted again.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportState {
    items: HashMap<String, ItemOutcome>,
}

impl ImportState {
    /// A fresh, empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads a job's state, or a fresh one if `path` does not exist yet (a first run).
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| ImportError::State {
                path: path.to_path_buf(),
                source: e,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ImportError::io(path, e)),
        }
    }

    /// Writes this state, through a temporary file and a rename.
    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).expect("import state always serialises");
        crate::atomic::write(path, &bytes).map_err(|e| ImportError::io(path, e))
    }

    /// What is recorded for `source_path`, if anything was attempted yet.
    pub fn outcome(&self, source_path: &str) -> Option<&ItemOutcome> {
        self.items.get(source_path)
    }

    /// Whether `source_path` needs no further work: already copied or already known skipped.
    pub fn is_settled(&self, source_path: &str) -> bool {
        matches!(
            self.items.get(source_path),
            Some(ItemOutcome::Copied | ItemOutcome::Skipped { .. })
        )
    }

    /// Records what happened to `source_path`.
    pub fn record(&mut self, source_path: impl Into<String>, outcome: ItemOutcome) {
        self.items.insert(source_path.into(), outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_state_file_is_a_fresh_state_not_an_error() {
        let dir = auroraw_testkit::temp_dir();
        let state = ImportState::load(&dir.path().join("job.json")).unwrap();
        assert!(state.outcome("a.raw").is_none());
    }

    #[test]
    fn recorded_outcomes_round_trip_and_settle_correctly() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("job.json");
        let mut state = ImportState::new();
        state.record("a.raw", ItemOutcome::Copied);
        state.record(
            "b.raw",
            ItemOutcome::Skipped {
                reason: "already imported (verified identical)".into(),
            },
        );
        state.record(
            "c.raw",
            ItemOutcome::Failed {
                error: "disk full".into(),
            },
        );
        state.save(&path).unwrap();

        let loaded = ImportState::load(&path).unwrap();
        assert!(loaded.is_settled("a.raw"));
        assert!(loaded.is_settled("b.raw"));
        assert!(
            !loaded.is_settled("c.raw"),
            "a failure is retried on resume"
        );
        assert!(!loaded.is_settled("d.raw"), "never attempted");
        assert_eq!(loaded.outcome("a.raw"), Some(&ItemOutcome::Copied));
    }
}
