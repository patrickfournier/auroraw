// SPDX-License-Identifier: GPL-3.0-or-later
//! Undo and Redo of what a person did to their photos (D-096). The engine owns the history; this object
//! only says what Undo and Redo would do (set from the engine's `historyChanged`, by the QML that connects
//! to the `Bus`) and sends the commands. The sentences ("Undo rating") are QML's, so that they are translated.

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
        /// What Undo would undo: `rating`, `flag`, `keywords` or `batch`; empty when nothing.
        #[qproperty(QString, undo_kind, cxx_name = "undoKind")]
        /// How many photos that step touched.
        #[qproperty(i32, undo_count, cxx_name = "undoCount")]
        /// What Redo would redo, as for `undoKind`.
        #[qproperty(QString, redo_kind, cxx_name = "redoKind")]
        /// How many photos that step touched.
        #[qproperty(i32, redo_count, cxx_name = "redoCount")]
        type History = super::HistoryRust;

        /// Undoes the last action, in the background.
        #[qinvokable]
        fn undo(self: &History);

        /// Redoes the action that was undone last, in the background.
        #[qinvokable]
        fn redo(self: &History);
    }
}

use auroraw_engine::Command;
use cxx_qt_lib::QString;

use crate::session;

/// The Rust side: the four properties.
#[derive(Default)]
pub struct HistoryRust {
    pub(crate) undo_kind: QString,
    pub(crate) undo_count: i32,
    pub(crate) redo_kind: QString,
    pub(crate) redo_count: i32,
}

impl qobject::History {
    pub fn undo(&self) {
        if let Some(session) = session::current() {
            let _ = session.engine.submit(Command::Undo);
        }
    }

    pub fn redo(&self) {
        if let Some(session) = session::current() {
            let _ = session.engine.submit(Command::Redo);
        }
    }
}
