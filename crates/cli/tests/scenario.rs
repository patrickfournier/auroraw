// SPDX-License-Identifier: GPL-3.0-or-later
//! The WP3 slice of the engine scenario (testing strategy §3): rate, keyword, edit a sidecar
//! from outside and confirm the change is picked up, rebuild, and compare - scripted entirely
//! through the real `auroraw-cli` binary, with no window. Import and versions are not built yet
//! (WP4, M2), so the fixture photo is seeded directly on disk, the way a future `add-folder`
//! command will.

use std::path::Path;
use std::process::{Command, Output};

use auroraw_format::sidecar::PhotoSidecar;
use auroraw_testkit::temp_dir;
use auroraw_types::PhotoId;
use auroraw_workspace::Workspace;

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_auroraw-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn ok(out: &Output) -> String {
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn seed_photo(workspace: &Path) -> PhotoId {
    let ws = Workspace::open(workspace).unwrap();
    let id = PhotoId::random();
    ws.write_photo(&PhotoSidecar::new(id)).unwrap();
    id
}

#[test]
fn the_wp3_engine_scenario_runs_end_to_end_through_the_cli() {
    let dir = temp_dir();
    let workspace = dir.path().join("Main");
    let catalogue = dir.path().join("main.sqlite");
    let ws = workspace.to_str().unwrap();
    let cat = catalogue.to_str().unwrap();

    let out = ok(&cli(&["create", ws, cat, "Main"]));
    assert!(out.contains("created"));

    let photo_id = seed_photo(&workspace);

    let out = ok(&cli(&["rebuild", ws, cat]));
    assert!(out.contains("rebuilt"));

    let out = ok(&cli(&["list", ws, cat]));
    assert!(out.contains(&photo_id.to_string()), "{out}");
    assert!(out.contains("rating=0"), "{out}");

    let out = ok(&cli(&["rate", ws, cat, &photo_id.to_string(), "4"]));
    assert!(out.contains("rated"));

    let out = ok(&cli(&["list", ws, cat, "--min-rating", "4"]));
    assert!(out.contains(&photo_id.to_string()), "{out}");
    assert!(out.contains("rating=4"), "{out}");

    let out = ok(&cli(&["list", ws, cat, "--min-rating", "5"]));
    assert!(!out.contains(&photo_id.to_string()), "{out}");

    let out = ok(&cli(&["flag", ws, cat, &photo_id.to_string(), "pick"]));
    assert!(out.contains("flagged"));
    let out = ok(&cli(&["list", ws, cat]));
    assert!(out.contains("flag=picked"), "{out}");

    let out = ok(&cli(&["keyword", "create", ws, cat, "Heron"]));
    let keyword_id = out
        .trim()
        .strip_prefix("created keyword ")
        .unwrap()
        .to_string();

    ok(&cli(&[
        "keyword",
        "add",
        ws,
        cat,
        &photo_id.to_string(),
        &keyword_id,
    ]));

    let out = ok(&cli(&[
        "keyword",
        "rename",
        ws,
        cat,
        &keyword_id,
        "Great Blue Heron",
    ]));
    assert!(out.contains("refreshing 1 sidecar"), "{out}");
    assert!(out.contains("refresh finished"), "{out}");

    let refreshed = Workspace::open(&workspace)
        .unwrap()
        .read_photo(&photo_id)
        .unwrap()
        .unwrap()
        .current()
        .unwrap();
    assert_eq!(
        refreshed.meta.keyword_paths,
        vec!["Great Blue Heron".to_string()]
    );

    // An edit made outside the engine (as if by another machine sharing the workspace): the
    // sidecar's rating changes on disk without going through the CLI at all.
    {
        let ws_handle = Workspace::open(&workspace).unwrap();
        let mut photo = ws_handle
            .read_photo(&photo_id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        photo.meta.rating = Some(2);
        ws_handle.write_photo(&photo).unwrap();
    }

    // Before verify, the catalogue still has the CLI's rating of 4: the outside edit has not
    // been picked up yet.
    let out = ok(&cli(&["list", ws, cat]));
    assert!(out.contains("rating=4"), "{out}");

    let out = ok(&cli(&["verify", ws, cat]));
    assert!(out.contains("verified"));

    let out = ok(&cli(&["list", ws, cat]));
    assert!(
        out.contains("rating=2"),
        "outside edit not reconciled: {out}"
    );

    // Rebuild from scratch must land on the same state a reconcile already reached.
    let before_rebuild = out;
    let out = ok(&cli(&["rebuild", ws, cat]));
    assert!(out.contains("rebuilt"));
    let after_rebuild = ok(&cli(&["list", ws, cat]));
    assert_eq!(before_rebuild, after_rebuild);

    ok(&cli(&[
        "keyword",
        "remove",
        ws,
        cat,
        &photo_id.to_string(),
        &keyword_id,
    ]));
}

#[test]
fn opening_a_missing_workspace_fails_cleanly() {
    let dir = temp_dir();
    let out = cli(&[
        "rebuild",
        dir.path().join("nope").to_str().unwrap(),
        dir.path().join("nope.sqlite").to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stderr).is_empty());
}
