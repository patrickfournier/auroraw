// SPDX-License-Identifier: GPL-3.0-or-later
//! Comparing the catalogue's record of a sidecar's size and modification time with what a fresh
//! scan finds, to decide which sidecars need to be re-read (architecture §5.3). This never reads
//! a sidecar itself: the caller does the scan and the reading; this only compares stats already
//! in hand against what the catalogue stored the last time it saw each file.

use std::collections::{HashMap, HashSet};

use auroraw_types::{PhotoId, VersionId};

use crate::SidecarStat;
use crate::error::Result;
use crate::open::Catalogue;

/// What a reconcile pass found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileReport<K> {
    /// New or changed: the caller should read the sidecar and write its row.
    pub changed: Vec<K>,
    /// In the catalogue but not in the current scan: the caller should mark it missing (D-019),
    /// never remove it silently.
    pub removed: Vec<K>,
}

impl Catalogue {
    /// Compares `current` (every photo sidecar the scan found, with its stat) against the
    /// catalogue's stored stats.
    pub fn reconcile_photos(
        &self,
        current: &[(PhotoId, SidecarStat)],
    ) -> Result<ReconcileReport<PhotoId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, sidecar_size, sidecar_modified FROM photo")?;
        let stored: HashMap<PhotoId, SidecarStat> = stmt
            .query_map([], |r| {
                let id: String = r.get(0)?;
                let size: i64 = r.get(1)?;
                Ok((
                    id,
                    SidecarStat {
                        size: size as u64,
                        modified: r.get(2)?,
                    },
                ))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, stat)| id.parse().ok().map(|id| (id, stat)))
            .collect();
        Ok(reconcile(current, &stored))
    }

    /// The same, for version sidecars, keyed by (photo, version).
    pub fn reconcile_versions(
        &self,
        current: &[((PhotoId, VersionId), SidecarStat)],
    ) -> Result<ReconcileReport<(PhotoId, VersionId)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT photo_id, id, sidecar_size, sidecar_modified FROM version")?;
        let stored: HashMap<(PhotoId, VersionId), SidecarStat> = stmt
            .query_map([], |r| {
                let photo: String = r.get(0)?;
                let version: String = r.get(1)?;
                let size: i64 = r.get(2)?;
                Ok((
                    photo,
                    version,
                    SidecarStat {
                        size: size as u64,
                        modified: r.get(3)?,
                    },
                ))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(p, v, stat)| Some(((p.parse().ok()?, v.parse().ok()?), stat)))
            .collect();
        Ok(reconcile(current, &stored))
    }
}

fn reconcile<K: std::hash::Hash + Eq + Copy>(
    current: &[(K, SidecarStat)],
    stored: &HashMap<K, SidecarStat>,
) -> ReconcileReport<K> {
    let mut changed = Vec::new();
    let mut seen = HashSet::with_capacity(current.len());
    for (key, stat) in current {
        seen.insert(*key);
        match stored.get(key) {
            Some(s) if s == stat => {}
            _ => changed.push(*key),
        }
    }
    let removed = stored
        .keys()
        .filter(|k| !seen.contains(*k))
        .copied()
        .collect();
    ReconcileReport { changed, removed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rebuild::{RebuildInput, build};
    use auroraw_format::sidecar::PhotoSidecar;
    use auroraw_types::WorkspaceId;

    fn photo(stat: SidecarStat) -> (PhotoSidecar, SidecarStat) {
        (PhotoSidecar::new(PhotoId::random()), stat)
    }

    #[test]
    fn unchanged_stats_report_nothing() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let photos = vec![photo(SidecarStat {
            size: 100,
            modified: Some(1),
        })];
        build(
            &mut cat,
            &RebuildInput {
                photos: &photos,
                ..Default::default()
            },
        )
        .unwrap();
        let current: Vec<_> = photos.iter().map(|(p, s)| (p.photo_id, *s)).collect();
        let report = cat.reconcile_photos(&current).unwrap();
        assert!(report.changed.is_empty() && report.removed.is_empty());
    }

    #[test]
    fn a_different_stat_is_changed_and_a_missing_one_is_removed() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let photos = vec![
            photo(SidecarStat {
                size: 100,
                modified: Some(1),
            }),
            photo(SidecarStat {
                size: 200,
                modified: Some(1),
            }),
        ];
        build(
            &mut cat,
            &RebuildInput {
                photos: &photos,
                ..Default::default()
            },
        )
        .unwrap();
        // the first grew, as if edited; the second is gone from the scan; a third is brand new
        let current = vec![
            (
                photos[0].0.photo_id,
                SidecarStat {
                    size: 150,
                    modified: Some(2),
                },
            ),
            (
                PhotoId::random(),
                SidecarStat {
                    size: 50,
                    modified: Some(1),
                },
            ),
        ];
        let new_id = current[1].0;
        let report = cat.reconcile_photos(&current).unwrap();
        assert_eq!(report.changed, vec![photos[0].0.photo_id, new_id]);
        assert_eq!(report.removed, vec![photos[1].0.photo_id]);
    }
}
