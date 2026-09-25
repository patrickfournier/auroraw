// SPDX-License-Identifier: GPL-3.0-or-later
//! The QML suites (`tests/qml/tst_*.qml`), run by QtQuickTest's runner in a process of their own, off
//! screen, with real key and mouse events. Each test here is one suite on a machine of its own;
//! the results are written to a file and must end in a clean totals line.

mod support;

use std::path::Path;
use std::process::Command;

use auroraw_testkit::temp_dir;

/// Runs `tests/qml/tst_<suite>.qml` with `home` as the machine, and fails with the suite's own
/// report when a test in it fails.
fn run_suite(suite: &str, home: &Path, extra: Option<&Path>) {
    let report = home.join(format!("{suite}.txt"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_qml-test-runner"));
    command
        // The module is built into the runner (its QML is in the executable's resources).
        .arg("-import")
        .arg("qrc:/qt/qml")
        .arg("-input")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/qml")
                .join(format!("tst_{suite}.qml")),
        )
        .arg("-o")
        .arg(format!("{},txt", report.display()))
        // The report goes to the console too, since a run that dies leaves the file empty.
        .arg("-o")
        .arg("-,txt")
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_QUICK_BACKEND", "software")
        .env("QT_QUICK_CONTROLS_STYLE", "Fusion")
        // The machine's own language must not change what the suites read.
        .env("LC_ALL", "C.UTF-8")
        .env("AURORAW_TEST_HOME", home);
    if let Some(extra) = extra {
        command.env("AURORAW_TEST_EXTRA", extra);
    }
    let output = command.output().expect("the runner starts");
    let text = std::fs::read_to_string(&report).unwrap_or_default();
    let clean = text
        .lines()
        .any(|line| line.starts_with("Totals: ") && line.contains(" 0 failed"));
    assert!(
        output.status.success() && clean,
        "suite {suite} failed ({}):\n{text}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The suites that make their own machines (folders under the one they are given).
#[test]
fn opening_and_creating_workspaces() {
    let home = temp_dir();
    run_suite("launch", home.path(), None);
}

#[test]
fn the_hamburger_menu_and_its_commands() {
    let home = temp_dir();
    run_suite("menus", home.path(), None);
}

#[test]
fn modal_dialogs_settings_and_the_language() {
    let home = temp_dir();
    run_suite("dialogs", home.path(), None);
}

#[test]
fn the_grid_on_a_machine_with_photos() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 80);
    let extra = home.path().join("Extra");
    support::write_photos(&extra, "EXTRA", 5);
    run_suite("grid", home.path(), Some(&extra));
}

/// A photo whose file has gone (an unplugged card, a deleted picture) is listed but has no thumbnail
/// to make: its cell says so, and the others show theirs.
#[test]
fn a_photo_that_no_thumbnail_can_be_made_for_says_so() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 12);
    std::fs::remove_file(home.path().join("Card").join("IMG_0005.jpg")).unwrap();
    run_suite("thumbnails", home.path(), None);
}

#[test]
fn a_scan_reaches_the_grid_and_makes_thumbnails_ahead() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 20);
    let extra = home.path().join("Extra");
    support::write_photos(&extra, "EXTRA", 5);
    run_suite("scan", home.path(), Some(&extra));
}
