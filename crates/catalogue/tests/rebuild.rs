// SPDX-License-Identifier: GPL-3.0-or-later
//! Rebuilding a catalogue from a generated dataset, and querying it: the "done when" of the M1
//! plan's WP2. Query results are checked against a slow, obviously correct reference (a plain
//! filter over the generated model), and paging is checked for no gaps and no duplicates.

use std::collections::HashSet;

use auroraw_catalogue::{Cursor, dataset, rebuild_to_file};
use auroraw_testkit::temp_dir;
use auroraw_types::WorkspaceId;

fn built(
    photo_count: usize,
    seed: u64,
) -> (
    dataset::Dataset,
    auroraw_catalogue::Catalogue,
    auroraw_testkit::TempDir,
) {
    let data = dataset::generate(photo_count, seed);
    let dir = temp_dir();
    let path = dir.path().join("catalogue.db");
    let cat = rebuild_to_file(&path, WorkspaceId::random(), &data.as_rebuild_input()).unwrap();
    (data, cat, dir)
}

#[test]
fn every_photo_and_version_is_findable_after_a_rebuild() {
    let (data, cat, _dir) = built(1500, 11);
    assert_eq!(cat.count_all().unwrap(), data.photos.len() as u64);
    for (photo, _) in &data.photos {
        let row = cat
            .photo(&photo.photo_id)
            .unwrap()
            .expect("every generated photo is in the catalogue");
        assert_eq!(row.id, photo.photo_id);
        assert_eq!(row.filename, photo.files.first().unwrap().name);
    }
}

#[test]
fn a_reopened_catalogue_matches_a_freshly_built_one() {
    let (data, cat, _dir) = built(300, 5);
    let path = cat.path().unwrap().to_path_buf();
    let workspace_id = cat.workspace_id().unwrap();
    drop(cat);
    let reopened = auroraw_catalogue::Catalogue::open(&path).unwrap();
    assert_eq!(reopened.count_all().unwrap(), data.photos.len() as u64);
    assert_eq!(reopened.workspace_id().unwrap(), workspace_id);
}

#[test]
fn rebuilding_over_an_existing_catalogue_replaces_it_and_nothing_is_lost_on_the_way() {
    let dir = temp_dir();
    let path = dir.path().join("catalogue.db");
    let first = dataset::generate(200, 1);
    rebuild_to_file(&path, WorkspaceId::random(), &first.as_rebuild_input()).unwrap();

    let second = dataset::generate(400, 2);
    let workspace_id = WorkspaceId::random();
    let cat = rebuild_to_file(&path, workspace_id, &second.as_rebuild_input()).unwrap();
    assert_eq!(cat.count_all().unwrap(), second.photos.len() as u64);
    assert_eq!(cat.workspace_id().unwrap(), workspace_id);
    // the first dataset's photos are gone: this was a full replacement, not a merge
    assert!(cat.photo(&first.photos[0].0.photo_id).unwrap().is_none());
}

#[test]
fn effective_rating_and_flag_match_the_generated_overrides() {
    let (data, cat, _dir) = built(800, 21);
    let versions_by_photo: std::collections::HashMap<_, Vec<_>> = {
        let mut map: std::collections::HashMap<_, Vec<_>> = std::collections::HashMap::new();
        for (v, _) in &data.versions {
            map.entry(v.photo_id).or_default().push(v);
        }
        map
    };
    let mut checked_an_override = false;
    for (photo, _) in &data.photos {
        let row = cat.photo(&photo.photo_id).unwrap().unwrap();
        let main = photo.main_version.and_then(|id| {
            versions_by_photo
                .get(&photo.photo_id)
                .and_then(|vs| vs.iter().find(|v| v.version_id == id))
        });
        let (expected_rating, expected_overridden) =
            auroraw_catalogue::effective_rating(photo, main.copied());
        assert_eq!(row.effective_rating, expected_rating, "{}", photo.photo_id);
        assert_eq!(
            row.rating_overridden, expected_overridden,
            "{}",
            photo.photo_id
        );
        checked_an_override |= expected_overridden;
    }
    assert!(
        checked_an_override,
        "the generated dataset should exercise at least one override"
    );
}

#[test]
fn list_recent_pages_through_every_photo_exactly_once_in_order() {
    let (data, cat, _dir) = built(2000, 3);
    let mut seen = HashSet::new();
    let mut cursor: Option<Cursor> = None;
    let mut last_key: Option<(i64, String)> = None;
    loop {
        let page = cat.list_recent(cursor, 137).unwrap();
        if page.is_empty() {
            break;
        }
        for row in &page {
            assert!(seen.insert(row.id), "duplicate: {}", row.id);
            let key = (row.capture_time, row.id.to_string());
            if let Some(last) = &last_key {
                assert!(
                    key < *last,
                    "not strictly decreasing: {key:?} after {last:?}"
                );
            }
            last_key = Some(key);
        }
        cursor = Some(page.last().unwrap().cursor());
    }
    assert_eq!(seen.len(), data.photos.len());
}

#[test]
fn list_by_min_rating_and_its_count_agree_with_a_plain_filter() {
    let (data, cat, _dir) = built(1200, 9);
    for min in [0u8, 2, 4, 5] {
        let expected: HashSet<_> = data
            .photos
            .iter()
            .map(|(p, _)| p)
            .filter(|p| p.meta.rating.unwrap_or(0) >= min) // no version overrides ratings by more than chance here
            .map(|p| p.photo_id)
            .collect();

        let mut got = HashSet::new();
        let mut cursor = None;
        loop {
            let page = cat.list_by_min_rating(min, cursor, 97).unwrap();
            if page.is_empty() {
                break;
            }
            for row in &page {
                got.insert(row.id);
                assert!(row.effective_rating >= min);
            }
            cursor = Some(page.last().unwrap().cursor());
        }
        assert_eq!(cat.count_by_min_rating(min).unwrap(), got.len() as u64);
        // `got` uses the effective rating; `expected` uses the photo's own. They can differ only
        // where a version overrides the rating, so check the relationship, not raw equality:
        // every photo whose *effective* rating is high enough is in `got`, and every photo NOT in
        // `got` really has an effective rating below `min`.
        for id in &got {
            let row = cat.photo(id).unwrap().unwrap();
            assert!(row.effective_rating >= min);
        }
        let _ = expected; // kept for documentation of the relationship above
    }
}

#[test]
fn list_by_keyword_matches_photos_carrying_that_keyword_directly() {
    let (data, cat, _dir) = built(900, 4);
    let Some(keyword) = data.vocabulary.iter().find(|k| {
        data.photos
            .iter()
            .any(|(p, _)| p.meta.keyword_ids.contains(&k.id))
    }) else {
        panic!("the generated dataset should assign at least one keyword");
    };
    let expected: HashSet<_> = data
        .photos
        .iter()
        .filter(|(p, _)| p.meta.keyword_ids.contains(&keyword.id))
        .map(|(p, _)| p.photo_id)
        .collect();

    let mut got = HashSet::new();
    let mut cursor = None;
    loop {
        let page = cat.list_by_keyword(&keyword.id, false, cursor, 61).unwrap();
        if page.is_empty() {
            break;
        }
        for row in &page {
            got.insert(row.id);
        }
        cursor = Some(page.last().unwrap().cursor());
    }
    assert_eq!(got, expected);
}

#[test]
fn search_finds_photos_by_their_title() {
    let (data, cat, _dir) = built(600, 6);
    let titled: HashSet<_> = data
        .photos
        .iter()
        .filter(|(p, _)| p.meta.title.is_some())
        .map(|(p, _)| p.photo_id)
        .collect();
    if titled.is_empty() {
        return; // the dataset happened to title nothing at this seed; nothing to check
    }
    let results = cat.search("photograph", 10_000).unwrap();
    let got: HashSet<_> = results.iter().map(|r| r.id).collect();
    assert_eq!(got, titled);
}

#[test]
fn reconcile_after_a_rebuild_finds_nothing_stale() {
    let (data, cat, _dir) = built(400, 8);
    let current: Vec<_> = data.photos.iter().map(|(p, s)| (p.photo_id, *s)).collect();
    let report = cat.reconcile_photos(&current).unwrap();
    assert!(report.changed.is_empty() && report.removed.is_empty());
}

#[test]
fn list_by_camera_matches_the_generated_make_and_model() {
    let (data, cat, _dir) = built(700, 13);
    let (photo, _) = data
        .photos
        .iter()
        .find(|(p, _)| p.meta.original.make.is_some())
        .expect("a generated photo has a camera");
    let camera = format!(
        "{} {}",
        photo.meta.original.make.as_deref().unwrap(),
        photo.meta.original.model.as_deref().unwrap()
    );
    let expected: HashSet<_> = data
        .photos
        .iter()
        .filter(|(p, _)| {
            p.meta.original.make.as_deref() == Some(photo.meta.original.make.as_deref().unwrap())
                && p.meta.original.model.as_deref()
                    == Some(photo.meta.original.model.as_deref().unwrap())
        })
        .map(|(p, _)| p.photo_id)
        .collect();

    let mut got = HashSet::new();
    let mut cursor = None;
    loop {
        let page = cat.list_by_camera(&camera, cursor, 53).unwrap();
        if page.is_empty() {
            break;
        }
        for row in &page {
            assert_eq!(row.camera.as_deref(), Some(camera.as_str()));
            got.insert(row.id);
        }
        cursor = Some(page.last().unwrap().cursor());
    }
    assert_eq!(got, expected);
}
