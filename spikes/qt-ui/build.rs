// SPDX-License-Identifier: GPL-3.0-or-later
use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("org.auroraw.spike").qml_files([
        "qml/Main.qml",
        "qml/Welcome.qml",
        "qml/NewWorkspaceDialog.qml",
        "qml/Library.qml",
    ]))
    .qt_module("Quick")
    .qt_module("QuickTest")
    .files(["src/launcher.rs", "src/grid.rs"])
    .cpp_file("src/thumbs.cpp")
    .build();
}
