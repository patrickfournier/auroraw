// SPDX-License-Identifier: GPL-3.0-or-later
//! The Slint user interface. It talks only to the engine (architecture §3.2): the types this
//! crate uses beyond `auroraw_engine` itself are plain data (`PhotoId`, `Thumbnail`) the engine's
//! own public API already hands back, the same way `cli` uses `auroraw_format`'s and
//! `auroraw_types`'s data types directly without depending on how the engine computes them.
//! Work package WP8: the shell, the welcome list, the library grid. Grows with later work packages
//! (cull mode, develop, publish, WP9 onward).

mod app;
mod app_settings;
pub mod commands;
mod grid;
#[cfg(test)]
mod headless_tests;
#[cfg(test)]
mod i18n_check;
mod settings;
mod workspace_shell;

// Slint's own generated code carries no doc comments; this crate's `missing_docs` lint (workspace
// wide) would otherwise warn on every item the macro below produces.
#[allow(missing_docs)]
mod generated {
    slint::include_modules!();
}

use std::path::PathBuf;
use std::rc::Rc;

use auroraw_engine::{Engine, LocalDirs, VolumeInfo};

/// What a launch needs from the application: where this machine keeps its data (this crate
/// resolves no directory itself, like every crate under `engine`), the Pictures folder new
/// workspaces are offered in, and optionally a workspace to open instead of the last one.
pub struct Launch {
    /// The data and cache folders.
    pub dirs: LocalDirs,
    /// The system's Pictures folder.
    pub pictures: PathBuf,
    /// A workspace folder named on the command line.
    pub open: Option<PathBuf>,
}

/// Asks a person for a folder: called with a dialog title, the folder to open the dialog at, and
/// what to do with the answer (`None` when the dialog was cancelled). Returns whether a dialog was
/// started. The answer is delivered on the interface thread. Tests without a display pass a
/// picker of their own: a native dialog cannot be opened without one.
pub type FolderPicker = Rc<dyn Fn(&str, Option<PathBuf>, Box<dyn FnOnce(Option<PathBuf>)>) -> bool>;

/// The removable volumes mounted right now.
pub type VolumeLister = Rc<dyn Fn() -> Vec<VolumeInfo>>;

/// What the interface asks of the machine it runs on, so the tests without a display can stand in
/// for it: the system's folder dialog and the list of mounted cards.
#[derive(Clone)]
pub struct Platform {
    /// The folder dialog.
    pub pick_folder: FolderPicker,
    /// The mounted removable volumes.
    pub volumes: VolumeLister,
}

impl Platform {
    /// This machine's own.
    pub fn native() -> Self {
        Self {
            pick_folder: native_folder_picker(),
            volumes: Rc::new(Engine::removable_volumes),
        }
    }
}

/// Opens the window (the last workspace, or the welcome list) and runs until the application is
/// quit or its window closed.
pub fn run(launch: Launch) -> Result<(), slint::PlatformError> {
    let _launcher = app::Launcher::start(launch, Platform::native())?;
    slint::run_event_loop_until_quit()
}

/// The system's own folder dialog (Windows and macOS: the native ones; Linux: the desktop portal),
/// run as a task of the event loop so the window stays alive and the dialog cannot freeze it.
fn native_folder_picker() -> FolderPicker {
    Rc::new(|title, start, done| {
        let title = title.to_string();
        slint::spawn_local(async move {
            let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
            if let Some(start) = start {
                dialog = dialog.set_directory(start);
            }
            done(
                dialog
                    .pick_folder()
                    .await
                    .map(|folder| folder.path().to_path_buf()),
            );
        })
        .is_ok()
    })
}

/// Where a folder dialog should open for what is typed in a field: the folder itself when it
/// exists, else the closest folder above it that does, else the system's default (`None`).
fn start_directory(typed: &str) -> Option<PathBuf> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    std::path::Path::new(typed)
        .ancestors()
        .find(|candidate| !candidate.as_os_str().is_empty() && candidate.is_dir())
        .map(std::path::Path::to_path_buf)
}
