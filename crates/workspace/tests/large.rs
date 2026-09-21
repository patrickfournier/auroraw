// SPDX-License-Identifier: GPL-3.0-or-later
//! A measurement, not a check: writes a workspace of many photos and versions, walks it, and
//! reads and parses every sidecar (M1 plan WP1, design note 001 §4, note 003 requirement 9).
//!
//! Run on demand, on each platform:
//! `AUR_LARGE=100000 cargo test -p auroraw-workspace --release --test large -- --ignored --nocapture`

use std::time::Instant;

use auroraw_format::sidecar::{FileEntry, FileRole, PhotoSidecar, VersionSidecar};
use auroraw_testkit::{rng, temp_dir};
use auroraw_types::{ContentHash, Fingerprint, KeywordId, PhotoId, VersionId};
use auroraw_workspace::Workspace;

fn threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16)
}

fn photo(r: &mut fastrand::Rng) -> PhotoSidecar {
    let mut bytes = [0u8; 16];
    bytes.iter_mut().for_each(|b| *b = r.u8(..));
    let mut p = PhotoSidecar::new(PhotoId::from_bytes(bytes));
    let m = &mut p.meta;
    m.rating = Some(r.u8(0..=5));
    m.title = Some("A photograph".into());
    m.caption = Some("A caption of a reasonable length for a photograph taken on a shoot".into());
    for i in 0..6 {
        m.push_keyword(
            KeywordId::from_bytes([i; 8]),
            format!("Category|Group {i}|Keyword {}", r.u32(..)),
        );
    }
    m.creator = vec!["A Photographer".into()];
    m.rights = Some("(c) 2026 A Photographer".into());
    m.original.capture_time = Some("2026-05-14T06:41:09.250-04:00".into());
    m.original.make = Some("SONY".into());
    m.original.model = Some("ILCE-7RM4".into());
    m.original.lens = Some("FE 24-70mm F2.8 GM".into());
    m.original.exposure_time = Some("1/250".into());
    m.original.f_number = Some("28/10".into());
    m.original.iso = vec!["400".into()];
    m.original.pixel_width = Some(9504);
    m.original.pixel_height = Some(6336);
    p.files.push(FileEntry {
        role: FileRole::Original,
        name: format!("DSC{:05}.ARW", r.u32(0..100_000)),
        format: Some("ARW".into()),
        size: 60_000_000,
        fingerprint: Fingerprint::from_bytes([r.u8(..); 32]),
        hash: Some(ContentHash::from_bytes([r.u8(..); 32])),
        locations: vec![],
        extra: vec![],
    });
    p
}

#[test]
#[ignore = "a measurement: run on demand with AUR_LARGE=<photos>"]
fn write_walk_and_read_a_large_workspace() {
    let n: usize = std::env::var("AUR_LARGE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);
    let dir = temp_dir();
    let ws = Workspace::create(&dir.path().join("Large"), "Large").unwrap();
    let mut r = rng(7);
    let mut photos = Vec::with_capacity(n);
    let mut versions = Vec::new();
    for _ in 0..n {
        let p = photo(&mut r);
        for _ in 0..[0, 1, 1, 1, 1, 1, 2, 3][r.usize(0..8)] {
            versions.push(VersionSidecar::new(&p, VersionId::random()));
        }
        photos.push(p);
    }
    let files = photos.len() + versions.len();

    let started = Instant::now();
    let chunk = files.div_ceil(threads()).max(1);
    std::thread::scope(|s| {
        for c in photos.chunks(chunk) {
            let ws = &ws;
            s.spawn(move || {
                for p in c {
                    ws.write_photo(p).unwrap();
                }
            });
        }
        for c in versions.chunks(chunk) {
            let ws = &ws;
            s.spawn(move || {
                for v in c {
                    ws.write_version(v).unwrap();
                }
            });
        }
    });
    let write = started.elapsed();

    let started = Instant::now();
    let scan = ws.scan().unwrap();
    let walk = started.elapsed();
    assert_eq!(scan.photos.len(), n);
    assert_eq!(scan.versions.len(), versions.len());
    assert!(scan.foreign.is_empty());

    let started = Instant::now();
    let ids: Vec<PhotoId> = scan.photos.iter().map(|e| e.key).collect();
    let keys: Vec<(PhotoId, VersionId)> = scan.versions.iter().map(|e| e.key).collect();
    let (pc, vc) = (
        ids.len().div_ceil(threads()).max(1),
        keys.len().div_ceil(threads()).max(1),
    );
    std::thread::scope(|s| {
        for c in ids.chunks(pc) {
            let ws = &ws;
            s.spawn(move || {
                c.iter()
                    .for_each(|id| assert!(ws.read_photo(id).unwrap().unwrap().current().is_some()))
            });
        }
        for c in keys.chunks(vc) {
            let ws = &ws;
            s.spawn(move || {
                c.iter().for_each(|(p, v)| {
                    assert!(ws.read_version(p, v).unwrap().unwrap().current().is_some())
                })
            });
        }
    });
    let read = started.elapsed();

    println!(
        "{{\"photos\": {n}, \"versions\": {}, \"files\": {files}, \"threads\": {}, \"write_s\": {:.2}, \"walk_ms\": {:.0}, \"read_parse_s\": {:.2}, \"read_parse_files_per_s\": {:.0}}}",
        versions.len(),
        threads(),
        write.as_secs_f64(),
        walk.as_secs_f64() * 1e3,
        read.as_secs_f64(),
        files as f64 / read.as_secs_f64()
    );
}
