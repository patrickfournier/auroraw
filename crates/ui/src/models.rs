// SPDX-License-Identifier: GPL-3.0-or-later
//! The list models: the library grid's (`PhotoGrid`, over the open workspace's photos; the selection is the
//! `GridView`'s and the moves are `gridmath`) and the
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
        #[qproperty(i32, min_rating, cxx_name = "minRating")]
        type PhotoGrid = super::PhotoGridRust;

        /// Loads the open workspace's photos, newest first, those rated `minRating` or more (the
        /// filter bar's choice; 0 lists them all).
        #[qinvokable]
        fn load(self: Pin<&mut PhotoGrid>);

        /// Lists only the photos rated `min_rating` or more.
        #[qinvokable]
        #[cxx_name = "filterBy"]
        fn filter_by(self: Pin<&mut PhotoGrid>, min_rating: i32);

        /// The photo in `row` (its identifier, empty when there is none).
        #[qinvokable]
        #[cxx_name = "idAt"]
        fn id_at(self: &PhotoGrid, row: i32) -> QString;

        /// The row of a photo, -1 when it is not listed.
        #[qinvokable]
        #[cxx_name = "rowOf"]
        fn row_of(self: &PhotoGrid, id: &QString) -> i32;

        /// Reads a photo's rating again from the catalogue (the engine says it changed) and redraws
        /// its cell when it is listed.
        #[qinvokable]
        #[cxx_name = "refreshPhoto"]
        fn refresh_photo(self: Pin<&mut PhotoGrid>, id: &QString);

        /// Reads a photo's rating again after an undo or a redo, at once: what the person asked for a
        /// moment ago no longer stands in for what the catalogue says.
        #[qinvokable]
        #[cxx_name = "syncPhoto"]
        fn sync_photo(self: Pin<&mut PhotoGrid>, id: &QString);

        /// What the status strip says of the photo in `row`: its file, its camera, its stars.
        #[qinvokable]
        #[cxx_name = "summaryAt"]
        fn summary_at(self: &PhotoGrid, row: i32) -> QString;

        /// Where a step of `(dx, dy)` cells from `current` lands, in rows of `columns`.
        #[qinvokable]
        fn step(self: &PhotoGrid, current: i32, dx: i32, dy: i32, columns: i32) -> i32;

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

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        #[qproperty(QString, job)]
        type SourceList = super::SourceListRust;

        /// Reads the open workspace's sources again.
        #[qinvokable]
        fn refresh(self: Pin<&mut SourceList>);

        /// The folder `text` names, as the application understands it (canonical); `error:` and
        /// why not.
        #[qinvokable]
        fn resolve(self: &SourceList, text: &QString) -> QString;

        /// What adding `folder` would do: `free`, `inside:<the source>`, or `contains:<the sources,
        /// quoted and separated by commas>`; `error:` and why not.
        #[qinvokable]
        fn plan(self: &SourceList, folder: &QString) -> QString;

        /// Adds `folder` as a source and starts its scan (`job` is its job); empty, or why not.
        /// `merge` takes the sources inside it into the new one.
        #[qinvokable]
        fn add(
            self: Pin<&mut SourceList>,
            folder: &QString,
            name: &QString,
            merge: bool,
        ) -> QString;

        /// How many photos the source in `row` has, and how many of them have work in them (read
        /// from the catalogue now, not from the list).
        #[qinvokable]
        #[cxx_name = "photosAt"]
        fn photos_at(self: &SourceList, row: i32) -> i32;
        #[qinvokable]
        #[cxx_name = "workedOnAt"]
        fn worked_on_at(self: &SourceList, row: i32) -> i32;

        /// Takes the source in `row` out of the catalogue, in the background; empty, or why not.
        #[qinvokable]
        fn remove(self: Pin<&mut SourceList>, row: i32) -> QString;

        /// Scans the source in `row` again; empty, or why not.
        #[qinvokable]
        fn rescan(self: Pin<&mut SourceList>, row: i32) -> QString;

        /// Scans the source whose folder is `folder` (an import made it a source); empty, or why not.
        #[qinvokable]
        #[cxx_name = "rescanFolder"]
        fn rescan_folder(self: Pin<&mut SourceList>, folder: &QString) -> QString;

        /// Answers the scan's question about photos removed earlier.
        #[qinvokable]
        #[cxx_name = "continueScan"]
        fn continue_scan(self: &SourceList, restore: bool);

        /// Stops the scan that waits at its question.
        #[qinvokable]
        #[cxx_name = "cancelScan"]
        fn cancel_scan(self: &SourceList);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut SourceList>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut SourceList>);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &SourceList, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &SourceList) -> QHash_i32_QByteArray;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &SourceList, _parent: &QModelIndex) -> i32;
    }
}

use core::pin::Pin;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant};

use auroraw_catalogue::Cursor;
use auroraw_engine::{Command, Engine, KnownWorkspace};
use auroraw_types::PhotoId;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};

use crate::session;
use crate::source_list::SourceListRust;

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
    min_rating: i32,
    items: Vec<Item>,
    /// Where each listed photo is.
    rows: HashMap<PhotoId, usize>,
    /// The ratings this grid asked the engine for and has not seen it confirm: until the catalogue
    /// says the same (or a moment passes), what it says is an older rating, not to be shown.
    pending: HashMap<PhotoId, (u8, Instant)>,
}

/// How long an unconfirmed rating is trusted over the catalogue (a command the engine refused).
const PENDING_FOR: Duration = Duration::from_secs(2);

/// Every photo the catalogue lists in the grid's own order (spike 3: the whole ordered list is cheap
/// even at 100,000 photos; only thumbnails are lazy), rated `min_rating` or more.
fn load_items(min_rating: u8) -> Vec<Item> {
    let Some(session) = session::current() else {
        return Vec::new();
    };
    let Ok(catalogue) = session.engine.read_catalogue() else {
        return Vec::new();
    };
    let mut items = Vec::new();
    let mut after = None;
    loop {
        let page = if min_rating == 0 {
            catalogue.list_recent(after, 5000)
        } else {
            catalogue.list_by_min_rating(min_rating, after, 5000)
        };
        let Ok(page) = page else {
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
        let mut items = load_items(self.min_rating.clamp(0, 5) as u8);
        // A rating asked for a moment ago may not be in the catalogue yet: it stays what was asked.
        self.as_mut()
            .rust_mut()
            .pending
            .retain(|_, (_, at)| at.elapsed() < PENDING_FOR);
        for item in &mut items {
            if let Some(&(asked, _)) = self.pending.get(&item.id) {
                item.rating = asked;
            }
        }
        let count = items.len() as i32;
        let rows = items
            .iter()
            .enumerate()
            .map(|(row, item)| (item.id, row))
            .collect();
        // SAFETY: every begin is followed by its end, with nothing in between that can fail.
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().items = items;
            self.as_mut().rust_mut().rows = rows;
            self.as_mut().end_reset_model();
        }
        self.set_count(count);
    }

    pub fn filter_by(mut self: Pin<&mut Self>, min_rating: i32) {
        self.as_mut().set_min_rating(min_rating.clamp(0, 5));
        self.load();
    }

    pub fn id_at(&self, row: i32) -> QString {
        usize::try_from(row)
            .ok()
            .and_then(|row| self.items.get(row))
            .map(|item| QString::from(item.id.to_string().as_str()))
            .unwrap_or_default()
    }

    pub fn row_of(&self, id: &QString) -> i32 {
        PhotoId::from_str(&id.to_string())
            .ok()
            .and_then(|id| self.rows.get(&id).copied())
            .map_or(-1, |row| row as i32)
    }

    pub fn sync_photo(mut self: Pin<&mut Self>, id: &QString) {
        if let Ok(photo) = PhotoId::from_str(&id.to_string()) {
            self.as_mut().rust_mut().pending.remove(&photo);
        }
        self.refresh_photo(id);
    }

    pub fn refresh_photo(mut self: Pin<&mut Self>, id: &QString) {
        let Ok(id) = PhotoId::from_str(&id.to_string()) else {
            return;
        };
        // A photo that is not listed (one that has just entered the catalogue) waits for the reload.
        let Some(&row) = self.rows.get(&id) else {
            return;
        };
        let Some(session) = session::current() else {
            return;
        };
        let Some(photo) = session
            .engine
            .read_catalogue()
            .ok()
            .and_then(|catalogue| catalogue.photo(&id).ok().flatten())
        else {
            return;
        };
        if let Some(&(asked, at)) = self.pending.get(&id) {
            if photo.effective_rating != asked && at.elapsed() < PENDING_FOR {
                // An older rating of a quick series of keys: the last one's own event follows.
                return;
            }
            self.as_mut().rust_mut().pending.remove(&id);
        }
        if self.items[row].rating != photo.effective_rating {
            self.as_mut().rust_mut().items[row].rating = photo.effective_rating;
            self.redraw_rating(row);
        }
    }

    fn redraw_rating(mut self: Pin<&mut Self>, row: usize) {
        let index = self.index(row as i32, 0, &QModelIndex::default());
        let mut roles = QVector::<i32>::default();
        roles.append(ROLE_RATING);
        self.as_mut().data_changed(&index, &index, &roles);
    }

    pub fn summary_at(&self, row: i32) -> QString {
        let Some(item) = usize::try_from(row)
            .ok()
            .and_then(|row| self.items.get(row))
        else {
            return QString::default();
        };
        let Some(session) = session::current() else {
            return QString::default();
        };
        let photo = session
            .engine
            .read_catalogue()
            .ok()
            .and_then(|catalogue| catalogue.photo(&item.id).ok().flatten());
        let Some(photo) = photo else {
            return QString::default();
        };
        // The file, the camera and the stars, as many of them as there are.
        let stars = "\u{2605}".repeat(usize::from(photo.rating));
        let parts: Vec<String> = [Some(photo.filename), photo.camera, Some(stars)]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect();
        QString::from(parts.join(" \u{2014} ").as_str())
    }

    pub fn step(&self, current: i32, dx: i32, dy: i32, columns: i32) -> i32 {
        crate::gridmath::step(
            current.max(0) as usize,
            dx,
            dy,
            columns.max(1) as usize,
            self.items.len(),
        ) as i32
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
        let Some(id) = usize::try_from(row)
            .ok()
            .and_then(|row| self.items.get(row))
            .map(|item| item.id)
        else {
            return;
        };
        let _ = session.engine.submit(Command::SetRating {
            photo_id: id,
            rating,
        });
        // The cell shows the new rating at once; the engine's own event confirms it.
        self.as_mut().rust_mut().items[row as usize].rating = rating;
        self.as_mut()
            .rust_mut()
            .pending
            .insert(id, (rating, Instant::now()));
        self.redraw_rating(row as usize);
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
