// SPDX-License-Identifier: GPL-3.0-or-later
//! Runs QtQuickTest suites against the interface's QML module (`tests/qml.rs` starts it, one process per
//! suite: a process can only have one application object). Not part of the application.

fn main() {
    std::process::exit(auroraw_ui_qt::quick_test());
}
