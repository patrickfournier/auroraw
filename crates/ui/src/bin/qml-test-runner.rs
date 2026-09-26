// SPDX-License-Identifier: GPL-3.0-or-later
//! Runs QtQuickTest suites against the interface's QML module (`tests/qml.rs` starts it, one process per
//! suite: a process can only have one application object). Not part of the application. With `--app` it
//! starts the application itself the way `auroraw-app` does, on the machine `AURORAW_TEST_HOME` names,
//! for a test that watches what the start-up says.

use std::path::PathBuf;

use auroraw_engine::LocalDirs;
use auroraw_ui::Launch;

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--app") {
        let launch = Launch {
            dirs: LocalDirs {
                data: PathBuf::from("data"),
                cache: PathBuf::from("cache"),
            },
            pictures: PathBuf::from("Pictures"),
            open: None,
        };
        std::process::exit(i32::from(auroraw_ui::run(launch).is_err()));
    }
    std::process::exit(auroraw_ui::quick_test());
}
