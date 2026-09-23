// SPDX-License-Identifier: GPL-3.0-or-later
//! What the interface needs to start an import from a folder or a card a person names (spec §5.2,
//! D-030: no pre-selection): finding or registering the two sources involved, deciding where the
//! job's resumable state lives, and submitting [`Command::Import`]. Kept here rather than in `ui`
//! so the same steps are testable without a window and reusable by a future CLI `import`.

use std::path::{Path, PathBuf};

use auroraw_import::Profile;
use auroraw_sources::filesystem::{LOCAL_FOLDER, REMOVABLE_VOLUME};
use auroraw_sources::volumes::{has_dcim, list_removable_volumes};
use auroraw_types::SourceId;

use crate::Engine;
use crate::command::Command;
use crate::coordinator::Outcome;
use crate::error::{EngineError, Result};
use crate::job::JobId;

/// A removable volume this machine has mounted right now (a memory card, a USB drive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeInfo {
    /// The device's name, for display.
    pub name: String,
    /// Where it is mounted.
    pub mount_point: PathBuf,
    /// Whether it looks like a camera's storage (a `DCIM` folder, M1 plan §6 item 6).
    pub has_dcim: bool,
}

/// Everything [`Engine::import`] needs beyond what the profile itself carries.
#[derive(Debug, Clone)]
pub struct ImportRequest {
    /// The card or folder to import from.
    pub source_root: PathBuf,
    /// The archive folder files are copied into (registered as a source on first use).
    pub archive_root: PathBuf,
    /// Destination template, pairing rule, metadata template.
    pub profile: Profile,
    /// A session name for the `{shoot}` template token.
    pub shoot: Option<String>,
    /// A second folder every file is also copied and verified into, laid out by the same
    /// destination template unless the profile names its own backup templates.
    pub backup_root: Option<PathBuf>,
    /// A folder, local to this machine, where the job's resumable state is kept.
    pub state_dir: PathBuf,
}

fn same_place(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn invalid(message: String) -> EngineError {
    EngineError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}

impl Engine {
    /// Every removable volume mounted right now, camera cards first.
    pub fn removable_volumes(&self) -> Vec<VolumeInfo> {
        let mut volumes: Vec<VolumeInfo> = list_removable_volumes()
            .into_iter()
            .map(|v| VolumeInfo {
                has_dcim: has_dcim(&v.mount_point),
                name: v.name,
                mount_point: v.mount_point,
            })
            .collect();
        volumes.sort_by_key(|v| !v.has_dcim);
        volumes
    }

    /// The source already registered for `root` on this machine, or a new one named `name` of
    /// `kind` (`local-folder` or `removable-volume`).
    pub fn find_or_add_source(&self, root: &Path, name: &str, kind: &str) -> Result<SourceId> {
        if let Some(sources) = self
            .workspace()
            .read_sources()?
            .and_then(|loaded| loaded.current())
            && let Some(entry) = sources.sources.iter().find(|entry| {
                entry
                    .hint
                    .get("path")
                    .and_then(|p| p.as_str())
                    .is_some_and(|p| same_place(Path::new(p), root))
            })
        {
            return Ok(entry.id);
        }
        match self.submit_and_wait(Command::AddSource {
            name: name.to_string(),
            root: root.to_path_buf(),
            kind: kind.to_string(),
        })? {
            Outcome::SourceAdded(id) => Ok(id),
            other => unreachable!("AddSource answers SourceAdded, not {other:?}"),
        }
    }

    /// Starts an import as a background job and returns its identifier at once; progress and the
    /// outcome arrive as `Event::ImportItem`, `Event::JobProgress` and `Event::ImportFinished`
    /// (or `Event::ImportAborted` when the source cannot be read at all).
    ///
    /// A job that was interrupted earlier from the same source resumes where it stopped (its
    /// state file is still there); one that finished without a failure removed its state, so a
    /// reformatted card that reuses the same file names is never mistaken for the last one.
    pub fn import(&self, request: ImportRequest) -> Result<JobId> {
        if !request.source_root.is_dir() {
            return Err(invalid(format!(
                "{} is not a folder",
                request.source_root.display()
            )));
        }
        if request.archive_root.as_os_str().is_empty() {
            return Err(invalid("no archive folder chosen".to_string()));
        }
        if same_place(&request.source_root, &request.archive_root) {
            return Err(invalid(
                "the archive folder is the folder being imported from".to_string(),
            ));
        }
        std::fs::create_dir_all(&request.archive_root)?;
        std::fs::create_dir_all(&request.state_dir)?;

        let removable = self
            .removable_volumes()
            .iter()
            .any(|v| same_place(&v.mount_point, &request.source_root));
        let source_name = request
            .source_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| request.source_root.display().to_string());
        let source_id = self.find_or_add_source(
            &request.source_root,
            &source_name,
            if removable {
                REMOVABLE_VOLUME
            } else {
                LOCAL_FOLDER
            },
        )?;
        let archive_id = self.find_or_add_source(&request.archive_root, "Archive", LOCAL_FOLDER)?;

        let mut profile = request.profile;
        let backup_roots = match request.backup_root {
            Some(root) => {
                std::fs::create_dir_all(&root)?;
                if profile.backup_templates.is_empty() {
                    profile.backup_templates = vec![profile.destination_template.clone()];
                }
                vec![root]
            }
            None => {
                profile.backup_templates.clear();
                Vec::new()
            }
        };
        match self.submit_and_wait(Command::Import {
            source_id,
            destination_source_id: archive_id,
            profile,
            shoot: request.shoot,
            backup_roots,
            state_path: request.state_dir.join(format!("import-{source_id}.json")),
        })? {
            Outcome::ImportStarted { job } => Ok(job),
            other => unreachable!("Import answers ImportStarted, not {other:?}"),
        }
    }
}
