// SPDX-License-Identifier: GPL-3.0-or-later
//! Where a folder dialog should open.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type Folders = super::FoldersRust;

        /// The folder a dialog should open at for what is typed in a field: the folder itself when it
        /// exists, else the closest folder above it that does, else an empty text (the system's own
        /// default place).
        #[qinvokable]
        fn closest(self: &Folders, typed: &QString) -> QString;
    }
}

use cxx_qt_lib::QString;

/// Holds nothing.
#[derive(Default)]
pub struct FoldersRust {}

/// See [`qobject::Folders::closest`].
pub fn closest_folder(typed: &str) -> Option<std::path::PathBuf> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    std::path::Path::new(typed)
        .ancestors()
        .find(|candidate| !candidate.as_os_str().is_empty() && candidate.is_dir())
        .map(std::path::Path::to_path_buf)
}

impl qobject::Folders {
    pub fn closest(&self, typed: &QString) -> QString {
        closest_folder(&typed.to_string())
            .map(|folder| QString::from(folder.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_opens_at_the_closest_folder_that_exists() {
        let dir = auroraw_testkit::temp_dir();
        let existing = dir.path().join("Photos");
        std::fs::create_dir_all(&existing).unwrap();

        assert_eq!(closest_folder(""), None);
        assert_eq!(closest_folder("   "), None);
        assert_eq!(
            closest_folder(&existing.to_string_lossy()),
            Some(existing.clone())
        );
        assert_eq!(
            closest_folder(&existing.join("2026/September/half-typed").to_string_lossy()),
            Some(existing),
            "the folders that do not exist yet are skipped"
        );
    }
}
