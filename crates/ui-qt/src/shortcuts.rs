// SPDX-License-Identifier: GPL-3.0-or-later
//! How a command's shortcut is written on this platform, for the menus.

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
        type Shortcuts = super::ShortcutsRust;

        /// An `Action.shortcut`, given as its `StandardKey` (else negative) and as text (else empty).
        #[qinvokable]
        fn text(self: &Shortcuts, standard: i32, sequence: &QString) -> QString;
    }
}

use cxx_qt_lib::QString;

use crate::glue;

/// Holds nothing.
#[derive(Default)]
pub struct ShortcutsRust {}

impl qobject::Shortcuts {
    pub fn text(&self, standard: i32, sequence: &QString) -> QString {
        QString::from(glue::shortcut_text(standard, &sequence.to_string()).as_str())
    }
}
