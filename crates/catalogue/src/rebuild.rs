// SPDX-License-Identifier: GPL-3.0-or-later
//! Building a catalogue from scratch, from a workspace's content (architecture §5.4, D-026).
//!
//! This module never touches a workspace: the caller (the `workspace` crate's scan, or a test)
//! reads every sidecar and state file and hands the parsed values here, each with the
//! [`SidecarStat`] the reconcile scan will later compare against. The result is written to a
//! temporary file and only then renamed over the target, so a rebuild that is interrupted never
//! leaves a half-built catalogue where the old one, or nothing, was (note 001 §5.4).

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use auroraw_format::sidecar::{PhotoSidecar, VersionSidecar};
use auroraw_format::state::{Collection, KeywordEntry, Series, SourceEntry};
use auroraw_types::{KeywordId, Timestamp, WorkspaceId};

use crate::SidecarStat;
use crate::error::{CatalogueError, Result};
use crate::open::Catalogue;

/// Everything a rebuild needs, already read and parsed by the caller.
#[derive(Debug, Default)]
pub struct RebuildInput<'a> {
    /// Every photo sidecar found, with its file's size and modification time.
    pub photos: &'a [(PhotoSidecar, SidecarStat)],
    /// Every version sidecar found.
    pub versions: &'a [(VersionSidecar, SidecarStat)],
    /// The keyword vocabulary.
    pub vocabulary: &'a [KeywordEntry],
    /// The sources.
    pub sources: &'a [SourceEntry],
    /// The collections, each with its file's stat.
    pub collections: &'a [(Collection, SidecarStat)],
    /// The series, each with its file's stat.
    pub series: &'a [(Series, SidecarStat)],
}

/// The full hierarchical path of every keyword (`"Fauna|Birds|Heron"`), resolved once by walking
/// the tree. A keyword whose parent chain is broken or cyclic (a corrupted vocabulary) still gets
/// a path: its own name, so the rebuild never drops a keyword for this reason alone.
fn keyword_paths(vocabulary: &[KeywordEntry]) -> HashMap<KeywordId, String> {
    let by_id: HashMap<KeywordId, &KeywordEntry> = vocabulary.iter().map(|k| (k.id, k)).collect();
    let mut paths = HashMap::with_capacity(vocabulary.len());
    for entry in vocabulary {
        let mut segments = vec![entry.name.as_str()];
        let mut current = entry.parent;
        let mut seen = std::collections::HashSet::new();
        seen.insert(entry.id);
        while let Some(parent_id) = current {
            if !seen.insert(parent_id) {
                break; // a cycle: stop rather than loop forever
            }
            match by_id.get(&parent_id) {
                Some(parent) => {
                    segments.push(parent.name.as_str());
                    current = parent.parent;
                }
                None => break, // a dangling parent (the vocabulary was edited concurrently)
            }
        }
        segments.reverse();
        paths.insert(entry.id, segments.join("|"));
    }
    paths
}

/// Populates an already-open (empty) catalogue from `input`, in one transaction.
pub(crate) fn build(cat: &mut Catalogue, input: &RebuildInput<'_>) -> Result<()> {
    let paths = keyword_paths(input.vocabulary);
    let versions_by_photo = crate::populate::group_versions_by_photo(input.versions);

    let tx = cat.conn.transaction()?;
    // The input's ordering is whatever its caller happened to hand over (a vocabulary read from
    // its state file is sorted by identifier, note 002, not parent-before-child; series and
    // photos can equally arrive in either order). Deferring foreign key checks to the commit,
    // rather than sorting every entity type topologically, lets the whole graph be inserted in
    // one pass regardless: SQLite still checks every reference, just once, at the end.
    tx.pragma_update(None, "defer_foreign_keys", "ON")?;
    for source in input.sources {
        crate::populate::insert_source(&tx, source)?;
    }
    for entry in input.vocabulary {
        let path = paths
            .get(&entry.id)
            .map(String::as_str)
            .unwrap_or(&entry.name);
        crate::populate::insert_keyword(&tx, entry, path)?;
    }
    for (series, stat) in input.series {
        crate::populate::insert_series(&tx, series, *stat)?;
    }
    for (collection, stat) in input.collections {
        crate::populate::insert_collection(&tx, collection, *stat)?;
    }
    for (photo, stat) in input.photos {
        let versions = versions_by_photo
            .get(&photo.photo_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        crate::populate::insert_photo(&tx, photo, *stat, versions)?;
    }
    for (version, stat) in input.versions {
        crate::populate::insert_version(&tx, version, *stat)?;
    }
    for (series, _) in input.series {
        for member in &series.members {
            crate::populate::set_photo_series(&tx, member, &series.id.to_string())?;
        }
    }
    tx.execute(
        "INSERT INTO meta(key, value) VALUES ('rebuilt_at', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [Timestamp::now().to_string()],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(windows)]
fn retryable(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::PermissionDenied)
        || matches!(e.raw_os_error(), Some(5 | 32 | 33))
}
#[cfg(not(windows))]
fn retryable(_: &std::io::Error) -> bool {
    false
}

/// Renames `from` to `to`, retrying briefly if another program has `to` open without allowing
/// deletion (Windows: antivirus scanners and search indexers do; note 001 §5.4).
fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut delays = [5u64, 10, 20, 40, 80, 160].into_iter();
    loop {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if retryable(&e) => match delays.next() {
                Some(ms) => std::thread::sleep(Duration::from_millis(ms)),
                None => return Err(e),
            },
            Err(e) => return Err(e),
        }
    }
}

fn remove_db_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

/// Builds a fresh catalogue at `path` from `input`, atomically: **nothing at `path` changes
/// until the whole build has succeeded**, so a crash or an error during the build leaves the
/// earlier catalogue (or nothing, the first time) exactly as it was. Only the final rename can
/// fail after the new catalogue is ready, and `std::fs::rename` replaces an existing destination
/// file as a single operation on every platform this project targets.
pub fn rebuild_to_file(
    path: &Path,
    workspace_id: WorkspaceId,
    input: &RebuildInput<'_>,
) -> Result<Catalogue> {
    let tmp = path.with_extension(match path.extension() {
        Some(ext) => format!("{}.rebuilding", ext.to_string_lossy()),
        None => "rebuilding".to_string(),
    });
    remove_db_files(&tmp);
    let result = (|| -> Result<()> {
        let mut cat = Catalogue::create(&tmp, workspace_id)?;
        build(&mut cat, input)?;
        // Fold the WAL into the main file and close, so only one file needs to be renamed and
        // the target never ends up beside a stale sibling from a different database.
        cat.conn.pragma_update(None, "journal_mode", "DELETE")?;
        cat.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        drop(cat);
        Ok(())
    })();
    if let Err(e) = result {
        remove_db_files(&tmp);
        return Err(e);
    }
    // The old catalogue at `path`, if any, stays untouched and valid until this line succeeds.
    rename_with_retry(&tmp, path).map_err(|e| CatalogueError::io(path, e))?;
    // Only now, with the new file safely in place, remove any `-wal`/`-shm` the old catalogue
    // left behind: they belong to a database that no longer exists at this path.
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
    Catalogue::open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use auroraw_types::PhotoId as Pid;

    #[test]
    fn keyword_paths_walk_the_tree_and_survive_a_cycle() {
        let root = KeywordEntry {
            id: KeywordId::from_bytes([1; 8]),
            name: "Fauna".into(),
            parent: None,
            synonyms: vec![],
            export: true,
            extra: Default::default(),
        };
        let child = KeywordEntry {
            id: KeywordId::from_bytes([2; 8]),
            name: "Heron".into(),
            parent: Some(root.id),
            synonyms: vec![],
            export: true,
            extra: Default::default(),
        };
        let cyclic = KeywordEntry {
            id: KeywordId::from_bytes([3; 8]),
            name: "Broken".into(),
            parent: Some(KeywordId::from_bytes([3; 8])),
            synonyms: vec![],
            export: true,
            extra: Default::default(),
        };
        let paths = keyword_paths(&[root.clone(), child.clone(), cyclic.clone()]);
        assert_eq!(paths[&root.id], "Fauna");
        assert_eq!(paths[&child.id], "Fauna|Heron");
        assert_eq!(
            paths[&cyclic.id], "Broken",
            "a cycle still gets a path, not an infinite loop"
        );
    }

    #[test]
    fn rebuild_to_file_never_leaves_a_half_built_catalogue() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("c.db");
        let photos = vec![(PhotoSidecar::new(Pid::random()), SidecarStat::default())];
        let input = RebuildInput {
            photos: &photos,
            ..Default::default()
        };
        let cat = rebuild_to_file(&path, WorkspaceId::random(), &input).unwrap();
        drop(cat);
        assert!(path.exists());
        for suffix in ["-wal", "-shm", ".rebuilding"] {
            assert!(
                !Path::new(&format!("{}{suffix}", path.display())).exists(),
                "{suffix}"
            );
        }
    }

    #[test]
    fn a_vocabulary_with_children_before_their_parents_still_builds() {
        // Exactly what `Vocabulary::to_bytes` produces (sorted by identifier, note 002): a child
        // can land before its parent. Found by the end-to-end test that writes and re-reads a
        // real workspace; kept here as a small, fast regression test.
        let parent = KeywordEntry {
            id: KeywordId::from_bytes([9; 8]),
            name: "Fauna".into(),
            parent: None,
            synonyms: vec![],
            export: true,
            extra: Default::default(),
        };
        let child = KeywordEntry {
            id: KeywordId::from_bytes([1; 8]),
            name: "Heron".into(),
            parent: Some(parent.id),
            synonyms: vec![],
            export: true,
            extra: Default::default(),
        };
        assert!(
            child.id < parent.id,
            "the ordering this test means to exercise"
        );
        let vocabulary = vec![child, parent];

        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("c.db");
        let input = RebuildInput {
            vocabulary: &vocabulary,
            ..Default::default()
        };
        rebuild_to_file(&path, WorkspaceId::random(), &input).unwrap();
    }

    #[test]
    fn a_failed_rebuild_leaves_the_old_catalogue_exactly_as_it_was() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("c.db");
        let good = vec![(PhotoSidecar::new(Pid::random()), SidecarStat::default())];
        rebuild_to_file(
            &path,
            WorkspaceId::random(),
            &RebuildInput {
                photos: &good,
                ..Default::default()
            },
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();

        // A version referencing a photo that is not in this input: with foreign keys deferred to
        // the commit (the fix above), this is exactly the kind of error that still must be
        // caught, and must not touch the catalogue that was already at `path`.
        let dangling = vec![(
            VersionSidecar::new(
                &PhotoSidecar::new(Pid::random()),
                auroraw_types::VersionId::random(),
            ),
            SidecarStat::default(),
        )];
        let err = rebuild_to_file(
            &path,
            WorkspaceId::random(),
            &RebuildInput {
                versions: &dangling,
                ..Default::default()
            },
        );
        assert!(err.is_err());

        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "the earlier catalogue is untouched"
        );
        for suffix in [
            "-wal",
            "-shm",
            ".rebuilding",
            ".rebuilding-wal",
            ".rebuilding-shm",
        ] {
            assert!(
                !Path::new(&format!("{}{suffix}", path.display())).exists(),
                "{suffix}"
            );
        }
    }
}
