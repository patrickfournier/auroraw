// SPDX-License-Identifier: GPL-3.0-or-later
//! The permanent rebuild test (M1 plan WP2 "done when", testing strategy §3): write a generated
//! dataset into a **real workspace** on disk, scan it, read every sidecar and state file back
//! (as the engine will, in WP3), and rebuild a catalogue from exactly that. Runs on the three
//! platforms through the ordinary per-change CI.

use std::time::SystemTime;

use auroraw_catalogue::{SidecarStat, dataset, rebuild_to_file};
use auroraw_format::state::{Sources, Vocabulary};
use auroraw_testkit::temp_dir;
use auroraw_types::{PhotoId, Timestamp, VersionId, WorkspaceId};
use auroraw_workspace::Workspace;

fn stat_of(stat: auroraw_workspace::FileStat) -> SidecarStat {
    let modified = stat
        .modified
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    SidecarStat {
        size: stat.size,
        modified,
    }
}

#[test]
fn a_workspace_written_to_disk_and_scanned_rebuilds_to_the_same_catalogue() {
    let data = dataset::generate(1500, 42);
    let dir = temp_dir();
    let ws = Workspace::create(&dir.path().join("Main"), "Main").unwrap();

    for (photo, _) in &data.photos {
        ws.write_photo(photo).unwrap();
    }
    for (version, _) in &data.versions {
        ws.write_version(version).unwrap();
    }
    ws.write_vocabulary(&Vocabulary {
        updated: Timestamp::now(),
        keywords: data.vocabulary.clone(),
        extra: Default::default(),
    })
    .unwrap();
    ws.write_sources(&Sources {
        updated: Timestamp::now(),
        sources: data.sources.clone(),
        extra: Default::default(),
    })
    .unwrap();
    for (collection, _) in &data.collections {
        ws.write_collection(collection).unwrap();
    }
    for (series, _) in &data.series {
        ws.write_series(series).unwrap();
    }

    // Scan the workspace and read every sidecar and state file back, the way the engine will.
    let scan = ws.scan().unwrap();
    assert!(
        scan.foreign.is_empty(),
        "nothing should be unrecognised: {:?}",
        scan.foreign
    );
    assert_eq!(scan.photos.len(), data.photos.len());
    assert_eq!(scan.versions.len(), data.versions.len());

    let photos: Vec<(auroraw_format::sidecar::PhotoSidecar, SidecarStat)> = scan
        .photos
        .iter()
        .map(|entry| {
            let sidecar = ws
                .read_photo(&entry.key)
                .unwrap()
                .unwrap()
                .current()
                .unwrap();
            (sidecar, stat_of(entry.stat.clone()))
        })
        .collect();
    let versions: Vec<(auroraw_format::sidecar::VersionSidecar, SidecarStat)> = scan
        .versions
        .iter()
        .map(|entry| {
            let (photo, version) = entry.key;
            let sidecar = ws
                .read_version(&photo, &version)
                .unwrap()
                .unwrap()
                .current()
                .unwrap();
            (sidecar, stat_of(entry.stat.clone()))
        })
        .collect();
    let vocabulary = ws
        .read_vocabulary()
        .unwrap()
        .unwrap()
        .current()
        .unwrap()
        .keywords;
    let sources = ws
        .read_sources()
        .unwrap()
        .unwrap()
        .current()
        .unwrap()
        .sources;
    let collections: Vec<_> = scan
        .collections
        .iter()
        .map(|entry| {
            let c = ws
                .read_collection(&entry.key)
                .unwrap()
                .unwrap()
                .current()
                .unwrap();
            (c, stat_of(entry.stat.clone()))
        })
        .collect();
    let series: Vec<_> = scan
        .series
        .iter()
        .map(|entry| {
            let s = ws
                .read_series(&entry.key)
                .unwrap()
                .unwrap()
                .current()
                .unwrap();
            (s, stat_of(entry.stat.clone()))
        })
        .collect();

    let input = auroraw_catalogue::RebuildInput {
        photos: &photos,
        versions: &versions,
        vocabulary: &vocabulary,
        sources: &sources,
        collections: &collections,
        series: &series,
    };
    let cat = rebuild_to_file(
        &dir.path().join("catalogue.db"),
        WorkspaceId::random(),
        &input,
    )
    .unwrap();

    assert_eq!(cat.count_all().unwrap(), data.photos.len() as u64);
    for (photo, _) in &data.photos {
        let row = cat
            .photo(&photo.photo_id)
            .unwrap()
            .expect("every written photo is in the catalogue");
        assert_eq!(row.id, photo.photo_id);
    }

    // Reconciling right after a rebuild, against the same real stats, finds nothing stale: the
    // sizes and times the catalogue stored are exactly what the disk has.
    let current: Vec<(PhotoId, SidecarStat)> = scan
        .photos
        .iter()
        .map(|e| (e.key, stat_of(e.stat.clone())))
        .collect();
    let report = cat.reconcile_photos(&current).unwrap();
    assert!(
        report.changed.is_empty() && report.removed.is_empty(),
        "{report:?}"
    );

    let current_versions: Vec<((PhotoId, VersionId), SidecarStat)> = scan
        .versions
        .iter()
        .map(|e| (e.key, stat_of(e.stat.clone())))
        .collect();
    let vreport = cat.reconcile_versions(&current_versions).unwrap();
    assert!(
        vreport.changed.is_empty() && vreport.removed.is_empty(),
        "{vreport:?}"
    );
}
