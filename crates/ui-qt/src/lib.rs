// SPDX-License-Identifier: GPL-3.0-or-later
//! The Qt Quick user interface (D-094; replaces the Slint `crates/ui` at cutover). It talks only to the
//! engine (architecture §3.2): the Rust objects here are thin, the screens are QML (`qml/`), and the
//! only C++ is `glue.cpp` (an image provider, the translation loader, the QtQuickTest entry).
//!
//! `unsafe` is confined to the modules that talk to C++ (`glue`, and the cxx-qt bridges): everything
//! else keeps the workspace's `deny` (D-094).

mod app_settings;
#[allow(unsafe_code)]
mod bus;
// The table is read by its own checks only, until a command palette reads it too.
#[cfg(test)]
mod commands;
#[allow(unsafe_code)]
mod files;
#[allow(unsafe_code)]
mod glue;
mod gridmath;
#[allow(unsafe_code)]
mod launcher;
#[allow(unsafe_code)]
mod models;
mod session;
#[allow(unsafe_code)]
mod shortcuts;
#[allow(unsafe_code)]
mod source_list;

use std::path::PathBuf;
use std::sync::Mutex;

use auroraw_engine::LocalDirs;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

mod translations {
    include!(concat!(env!("OUT_DIR"), "/translations.rs"));
}

/// The compiled translation for a language code (`None` when there is none: the source texts).
pub(crate) fn translation_for(code: &str) -> Option<&'static [u8]> {
    translations::TRANSLATIONS
        .iter()
        .find(|(language, _)| *language == code)
        .map(|(_, bytes)| *bytes)
}

/// What a launch needs from the application: where this machine keeps its data (this crate resolves no
/// directory itself, like every crate under `engine`), the Pictures folder new workspaces are offered
/// in, and optionally a workspace to open instead of the last one.
#[derive(Debug, Clone)]
pub struct Launch {
    /// The data and cache folders.
    pub dirs: LocalDirs,
    /// The system's Pictures folder.
    pub pictures: PathBuf,
    /// A workspace folder named on the command line.
    pub open: Option<PathBuf>,
}

static LAUNCH: Mutex<Option<Launch>> = Mutex::new(None);

/// A sub-folder of `AURORAW_TEST_HOME` a test chose as its machine (`Launcher.useMachine`).
static MACHINE: Mutex<Option<String>> = Mutex::new(None);

/// Chooses the machine the tests run on: a folder named `name` under `AURORAW_TEST_HOME` (the folder
/// itself for an empty name).
pub(crate) fn use_machine(name: &str) {
    *MACHINE.lock().unwrap() = Some(name.to_string());
}

/// The launch in force. `AURORAW_TEST_HOME`, when set, moves the data, cache and Pictures folders
/// under it (a hook for the tests, which each get a machine of their own).
pub(crate) fn launch() -> Launch {
    let mut launch = LAUNCH
        .lock()
        .unwrap()
        .clone()
        .expect("run() or quick_test() stored the launch");
    if let Some(home) = std::env::var_os("AURORAW_TEST_HOME") {
        let mut home = PathBuf::from(home);
        if let Some(name) = MACHINE.lock().unwrap().as_deref() {
            home = home.join(name);
        }
        launch.dirs = LocalDirs {
            data: home.join("data"),
            cache: home.join("cache"),
        };
        launch.pictures = home.join("Pictures");
    }
    launch
}

/// Why the interface stopped with an error.
#[derive(Debug)]
pub struct UiError(pub String);

impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UiError {}

/// Forces the linker to keep what cxx-qt registers at start-up (the QML module and this crate's
/// objects), which a library crate would otherwise lose.
fn keep_registrations() {
    cxx_qt::init_crate!(auroraw_ui_qt);
    cxx_qt::init_qml_module!("org.auroraw.ui");
}

/// The application's look and language, before any screen exists: Fusion on every platform (D-094,
/// no native styles), and the language the settings (or the machine) ask for.
fn prepare_style() {
    QQuickStyle::set_style(&QString::from("Fusion"));
}

/// Opens the window (the last workspace, or the welcome list) and runs until the application is quit
/// or its window closed.
pub fn run(launch: Launch) -> Result<(), UiError> {
    keep_registrations();
    *LAUNCH.lock().unwrap() = Some(launch);
    prepare_style();
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    let Some(engine) = engine.as_mut() else {
        return Err(UiError("the QML engine could not be created".into()));
    };
    engine.load(&QUrl::from("qrc:/qt/qml/org/auroraw/ui/qml/Main.qml"));
    match app.as_mut() {
        Some(app) => {
            app.exec();
            Ok(())
        }
        None => Err(UiError(
            "the application object could not be created".into(),
        )),
    }
}

/// Runs the QtQuickTest suites named on the command line against the QML module (the test runner
/// binary calls this); returns the number of failures. Each suite gets a machine of its own through
/// `AURORAW_TEST_HOME`.
#[cfg(feature = "quicktest")]
pub fn quick_test() -> i32 {
    keep_registrations();
    *LAUNCH.lock().unwrap() = Some(Launch {
        dirs: LocalDirs {
            data: PathBuf::from("unused"),
            cache: PathBuf::from("unused"),
        },
        pictures: PathBuf::from("unused"),
        open: None,
    });
    prepare_style();
    glue::quick_test(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/qml"))
}
