// SPDX-License-Identifier: GPL-3.0-or-later
//! The list models: the library grid's (`PhotoGrid`, over the open workspace's photos; the Qt-side twin
//! of `crates/ui/src/grid.rs`, whose selection and paging keys are the `GridView`'s here) and the
//! welcome list's (`KnownWorkspaces`, over the registry). Both are `QAbstractListModel`s, and the base
//! class can only be declared once per link, so they share this bridge.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        #[qobject]
        type QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        type PhotoGrid = super::PhotoGridRust;

        /// Loads the open workspace's photos, newest first.
        #[qinvokable]
        fn load(self: Pin<&mut PhotoGrid>);

        /// Where a keyboard move lands (`page-up`, `page-down`, `home` or `end`), given the selected
        /// index, the columns and the rows that fit on screen.
        #[qinvokable]
        fn jump(self: &PhotoGrid, kind: &QString, current: i32, columns: i32, rows: i32) -> i32;

        /// Rates the photo in `row` (0 clears).
        #[qinvokable]
        #[cxx_name = "setRating"]
        fn set_rating(self: Pin<&mut PhotoGrid>, row: i32, rating: i32);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut PhotoGrid>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut PhotoGrid>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut PhotoGrid>);

        #[inherit]
        fn index(self: &PhotoGrid, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &PhotoGrid, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &PhotoGrid) -> QHash_i32_QByteArray;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &PhotoGrid, _parent: &QModelIndex) -> i32;
    }
    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        type KnownWorkspaces = super::KnownWorkspacesRust;

        /// Reads the registry again.
        #[qinvokable]
        fn refresh(self: Pin<&mut KnownWorkspaces>);

        /// The folder of the workspace in `row`.
        #[qinvokable]
        #[cxx_name = "pathAt"]
        fn path_at(self: &KnownWorkspaces, row: i32) -> QString;

        /// Takes the workspace in `row` off the list (its folder is not touched).
        #[qinvokable]
        fn forget(self: Pin<&mut KnownWorkspaces>, row: i32);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut KnownWorkspaces>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut KnownWorkspaces>);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &KnownWorkspaces, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &KnownWorkspaces) -> QHash_i32_QByteArray;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &KnownWorkspaces, _parent: &QModelIndex) -> i32;
    }

    // Reads the registry once the object exists.
    impl cxx_qt::Initialize for KnownWorkspaces {}
}

use core::pin::Pin;

use auroraw_catalogue::Cursor;
use auroraw_engine::{Command, Engine, KnownWorkspace};
use auroraw_types::PhotoId;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};

use crate::session;

/// Qt::UserRole and the next one.
const ROLE_PHOTO_ID: i32 = 0x0100;
const ROLE_RATING: i32 = 0x0101;

struct Item {
    id: PhotoId,
    rating: u8,
}

/// The Rust side of the grid model.
#[derive(Default)]
pub struct PhotoGridRust {
    count: i32,
    items: Vec<Item>,
}

fn load_items() -> Vec<Item> {
    let Some(session) = session::current() else {
        return Vec::new();
    };
    let Ok(catalogue) = session.engine.read_catalogue() else {
        return Vec::new();
    };
    let mut items = Vec::new();
    let mut after = None;
    loop {
        let Ok(page) = catalogue.list_recent(after, 5000) else {
            break;
        };
        if page.is_empty() {
            break;
        }
        after = page.last().map(|row| Cursor {
            capture_time: row.capture_time,
            id: row.id,
        });
        items.extend(page.into_iter().map(|row| Item {
            id: row.id,
            rating: row.effective_rating,
        }));
    }
    items
}

impl qobject::PhotoGrid {
    pub fn load(mut self: Pin<&mut Self>) {
        let items = load_items();
        let count = items.len() as i32;
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().items = items;
            self.as_mut().end_reset_model();
        }
        self.set_count(count);
    }

    pub fn jump(&self, kind: &QString, current: i32, columns: i32, rows: i32) -> i32 {
        crate::gridmath::jump(
            &kind.to_string(),
            current.max(0) as usize,
            columns.max(1) as usize,
            rows.max(1) as usize,
            self.items.len(),
        ) as i32
    }

    pub fn set_rating(mut self: Pin<&mut Self>, row: i32, rating: i32) {
        let Some(session) = session::current() else {
            return;
        };
        let rating = rating.clamp(0, 5) as u8;
        let Some(id) = self.items.get(row as usize).map(|item| item.id) else {
            return;
        };
        let _ = session.engine.submit(Command::SetRating {
            photo_id: id,
            rating,
        });
        self.as_mut().rust_mut().items[row as usize].rating = rating;
        let index = self.index(row, 0, &QModelIndex::default());
        let mut roles = QVector::<i32>::default();
        roles.append(ROLE_RATING);
        self.as_mut().data_changed(&index, &index, &roles);
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(item) = self.items.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_PHOTO_ID => QVariant::from(&QString::from(item.id.to_string().as_str())),
            ROLE_RATING => QVariant::from(&i32::from(item.rating)),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_PHOTO_ID, QByteArray::from("photoId"));
        roles.insert(ROLE_RATING, QByteArray::from("rating"));
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.items.len() as i32
    }
}

/// Qt::UserRole and the next ones.
const ROLE_NAME: i32 = 0x0100;
const ROLE_PATH: i32 = 0x0101;
const ROLE_OPENED: i32 = 0x0102;
const ROLE_FOUND: i32 = 0x0103;

/// The Rust side of the model.
#[derive(Default)]
pub struct KnownWorkspacesRust {
    count: i32,
    known: Vec<KnownWorkspace>,
}

impl cxx_qt::Initialize for qobject::KnownWorkspaces {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}

impl qobject::KnownWorkspaces {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let known = Engine::known_workspaces(&crate::launch().dirs);
        let count = known.len() as i32;
        // SAFETY: every begin is followed by its end, with nothing in between that can fail.
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().known = known;
            self.as_mut().end_reset_model();
        }
        self.set_count(count);
    }

    pub fn path_at(&self, row: i32) -> QString {
        self.known
            .get(row as usize)
            .map(|k| QString::from(k.path.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }

    pub fn forget(mut self: Pin<&mut Self>, row: i32) {
        if let Some(entry) = self.known.get(row as usize) {
            let _ = Engine::forget_workspace(&crate::launch().dirs, entry.workspace_id);
        }
        self.as_mut().refresh();
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(entry) = self.known.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_NAME => QVariant::from(&QString::from(entry.name.as_str())),
            ROLE_PATH => QVariant::from(&QString::from(entry.path.to_string_lossy().as_ref())),
            ROLE_OPENED => QVariant::from(&QString::from(
                entry
                    .opened
                    .map(|when| when.to_string().chars().take(10).collect::<String>())
                    .unwrap_or_default()
                    .as_str(),
            )),
            ROLE_FOUND => QVariant::from(&entry.found),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_NAME, QByteArray::from("name"));
        roles.insert(ROLE_PATH, QByteArray::from("path"));
        roles.insert(ROLE_OPENED, QByteArray::from("opened"));
        roles.insert(ROLE_FOUND, QByteArray::from("found"));
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.known.len() as i32
    }
}
