// SPDX-License-Identifier: GPL-3.0-or-later
//! Qt Quick spike: the first screens of the interface through cxx-qt.

mod fixture;
mod grid;
mod launcher;
mod session;
mod thumbs;

use std::ffi::c_void;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--make-fixture") => return fixture::make(),
        Some("--quicktest") => return thumbs::quick_test(),
        _ => {}
    }
    let mut app = QGuiApplication::new();
    // The language: `SPIKE_QM` names a compiled translation (`lrelease`), else the source texts.
    if let Some(qm) = std::env::var_os("SPIKE_QM") {
        thumbs::install_translation(&qm.to_string_lossy());
    }
    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        // SAFETY: the pointer is only handed to the provider registration, during this call.
        let raw = unsafe { engine.as_mut().get_unchecked_mut() as *mut QQmlApplicationEngine };
        thumbs::register(raw as *mut c_void);
        engine.load(&QUrl::from("qrc:/qt/qml/org/auroraw/spike/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
