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
        .arg("-input")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/qml")
                .join(format!("tst_{suite}.qml")),
        )
        .arg("-o")
        .arg(format!("{},txt", report.display()))
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_QUICK_BACKEND", "software")
        .env("QT_QUICK_CONTROLS_STYLE", "Fusion")
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
        "suite {suite} failed ({}):\n{text}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn modality_and_menus_on_an_empty_machine() {
    let home = temp_dir();
    run_suite("modality", home.path(), None);
}

#[test]
fn the_grid_on_a_machine_with_photos() {
    let home = temp_dir();
    support::machine_with_photos(home.path(), 80);
    let extra = home.path().join("Extra");
    support::write_photos(&extra, "EXTRA", 5);
    run_suite("grid", home.path(), Some(&extra));
}

#[test]
fn the_language_can_be_switched_live() {
    let home = temp_dir();
    run_suite("language", home.path(), None);
}
