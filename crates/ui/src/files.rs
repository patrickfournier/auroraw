// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests: a few file system helpers for the QML suites, which cannot touch files themselves (make a
//! folder, move a workspace out of sight, read a settings file...).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type Files = super::FilesRust;

        /// An environment variable.
        #[qinvokable]
        fn env(self: &Files, name: &QString) -> QString;

        #[qinvokable]
        fn exists(self: &Files, path: &QString) -> bool;

        #[qinvokable]
        fn mkdir(self: &Files, path: &QString);

        #[qinvokable]
        fn write(self: &Files, path: &QString, text: &QString);

        #[qinvokable]
        fn read(self: &Files, path: &QString) -> QString;

        /// Moves a folder, once: a workspace that has just closed may not have let go of it yet
        /// (Windows), and letting go takes the event loop, so tests retry with `tryVerify`.
        #[qinvokable]
        fn rename(self: &Files, from: &QString, to: &QString) -> bool;

        /// Copies the files of the folder `from` into the folder `to` (made if need be).
        #[qinvokable]
        #[cxx_name = "copyDir"]
        fn copy_dir(self: &Files, from: &QString, to: &QString);

        /// Deletes a file.
        #[qinvokable]
        fn remove(self: &Files, path: &QString);

        /// Copies one file.
        #[qinvokable]
        #[cxx_name = "copyFile"]
        fn copy_file(self: &Files, from: &QString, to: &QString);

        /// The canonical form of a path, as the application understands it.
        #[qinvokable]
        fn canonical(self: &Files, path: &QString) -> QString;

        /// How many thumbnails the previews database of the only workspace under the cache folder
        /// `cache` holds (-1 when there is none).
        #[qinvokable]
        #[cxx_name = "previewsCount"]
        fn previews_count(self: &Files, cache: &QString) -> i32;
    }
}

use std::path::PathBuf;

use auroraw_engine::paths;
use cxx_qt_lib::QString;

/// The Rust side of the helpers: it holds nothing.
#[derive(Default)]
pub struct FilesRust {}

fn path(text: &QString) -> PathBuf {
    PathBuf::from(text.to_string())
}

impl qobject::Files {
    pub fn env(&self, name: &QString) -> QString {
        QString::from(std::env::var(name.to_string()).unwrap_or_default().as_str())
    }

    pub fn exists(&self, target: &QString) -> bool {
        path(target).exists()
    }

    pub fn mkdir(&self, target: &QString) {
        let _ = std::fs::create_dir_all(path(target));
    }

    pub fn write(&self, target: &QString, text: &QString) {
        if let Some(parent) = path(target).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path(target), text.to_string());
    }

    pub fn read(&self, target: &QString) -> QString {
        QString::from(
            std::fs::read_to_string(path(target))
                .unwrap_or_default()
                .as_str(),
        )
    }

    pub fn rename(&self, from: &QString, to: &QString) -> bool {
        std::fs::rename(path(from), path(to)).is_ok()
    }

    pub fn copy_dir(&self, from: &QString, to: &QString) {
        let _ = std::fs::create_dir_all(path(to));
        for entry in std::fs::read_dir(path(from))
            .into_iter()
            .flatten()
            .flatten()
        {
            if entry.path().is_file() {
                let _ = std::fs::copy(entry.path(), path(to).join(entry.file_name()));
            }
        }
    }

    pub fn remove(&self, target: &QString) {
        let _ = std::fs::remove_file(path(target));
    }

    pub fn copy_file(&self, from: &QString, to: &QString) {
        if let Some(parent) = path(to).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::copy(path(from), path(to));
    }

    pub fn previews_count(&self, cache: &QString) -> i32 {
        let Some(previews) = std::fs::read_dir(path(cache).join("catalogues"))
            .ok()
            .and_then(|mut entries| entries.next())
            .and_then(Result::ok)
            .map(|entry| entry.path().join("previews.db"))
        else {
            return -1;
        };
        auroraw_imaging::PreviewsDb::open(&previews)
            .and_then(|db| db.count())
            .map_or(-1, |count| count as i32)
    }

    pub fn canonical(&self, target: &QString) -> QString {
        let resolved = paths::resolve(&target.to_string()).unwrap_or_default();
        QString::from(resolved.to_string_lossy().as_ref())
    }
}
