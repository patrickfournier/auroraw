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
        #[qproperty(i32, selected_count, cxx_name = "selectedCount")]
        #[qproperty(i32, flag_filter, cxx_name = "flagFilter")]
        #[qproperty(QString, keyword_filter, cxx_name = "keywordFilter")]
        type PhotoGrid = super::PhotoGridRust;

        /// Loads the open workspace's photos, newest first, those rated `minRating` or more (the
        /// filter bar's choice; 0 lists them all).
        #[qinvokable]
        fn load(self: Pin<&mut PhotoGrid>);

        /// Lists only the photos rated `min_rating` or more.
        #[qinvokable]
        #[cxx_name = "filterBy"]
        fn filter_by(self: Pin<&mut PhotoGrid>, min_rating: i32);

        /// Lists photos by flag: 0 everything but the rejected (the default), 1 all, 2 picked, 3 rejected.
        /// The rating and keyword filters stay; nothing is selected any more.
        #[qinvokable]
        #[cxx_name = "filterFlags"]
        fn filter_flags(self: Pin<&mut PhotoGrid>, flags: i32);

        /// Lists only the photos with this keyword or one under it (an identifier; empty for no keyword
        /// filter). The other filters stay; nothing is selected any more.
        #[qinvokable]
        #[cxx_name = "filterKeyword"]
        fn filter_keyword(self: Pin<&mut PhotoGrid>, keyword: &QString);

        /// Flags the selection as one action: `pick`, `reject` (the flag is cleared instead when every
        /// selected photo has it already) or `clear`. How many photos were changed.
        #[qinvokable]
        #[cxx_name = "flagSelection"]
        fn flag_selection(self: Pin<&mut PhotoGrid>, kind: &QString) -> i32;

        /// For each keyword a selected photo carries, how many selected photos carry it, as JSON
        /// (`{"<keyword id>": 3}`): what the keyword panel shows as none, some or all.
        #[qinvokable]
        #[cxx_name = "keywordUsage"]
        fn keyword_usage(self: &PhotoGrid) -> QString;

        /// Adds a keyword (an identifier) to every selected photo, or removes it, as one action. How
        /// many photos.
        #[qinvokable]
        #[cxx_name = "keywordSelection"]
        fn keyword_selection(self: Pin<&mut PhotoGrid>, keyword: &QString, add: bool) -> i32;

        /// Makes the keyword `name` under `parent` (an identifier; empty for the top level) and gives it to
        /// every selected photo, as one action (one step of the history). Its identifier, or `error:` and
        /// why not.
        #[qinvokable]
        #[cxx_name = "createKeywordSelection"]
        fn create_keyword_selection(
            self: Pin<&mut PhotoGrid>,
            name: &QString,
            parent: &QString,
        ) -> QString;

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

        /// Selects only the photo in `row`, and anchors ranges there (a click, an arrow).
        #[qinvokable]
        #[cxx_name = "selectOnly"]
        fn select_only(self: Pin<&mut PhotoGrid>, row: i32);

        /// Adds the photo in `row` to the selection, or removes it (Ctrl+click, Space), and anchors there.
        #[qinvokable]
        fn toggle(self: Pin<&mut PhotoGrid>, row: i32);

        /// Selects the photos from the anchor to `row`: replacing the selection (Shift), or added to it
        /// (`additive`, Ctrl+Shift). The anchor stays.
        #[qinvokable]
        #[cxx_name = "extendTo"]
        fn extend_to(self: Pin<&mut PhotoGrid>, row: i32, additive: bool);

        /// Selects every photo listed.
        #[qinvokable]
        #[cxx_name = "selectAll"]
        fn select_all(self: Pin<&mut PhotoGrid>);

        /// Selects nothing.
        #[qinvokable]
        #[cxx_name = "selectNone"]
        fn select_none(self: Pin<&mut PhotoGrid>);

        /// Selects the photos that are not selected, and only those.
        #[qinvokable]
        fn invert(self: Pin<&mut PhotoGrid>);

        /// Selects exactly these photos (identifiers joined by commas), those that are listed.
        #[qinvokable]
        #[cxx_name = "selectPhotos"]
        fn select_photos(self: Pin<&mut PhotoGrid>, ids: &QString);

        /// Whether the photo in `row` is selected.
        #[qinvokable]
        #[cxx_name = "isSelected"]
        fn is_selected(self: &PhotoGrid, row: i32) -> bool;

        /// The first selected row, -1 when nothing is selected.
        #[qinvokable]
        #[cxx_name = "firstSelectedRow"]
        fn first_selected_row(self: &PhotoGrid) -> i32;

        /// The row ranges start from, -1 when there is none.
        #[qinvokable]
        #[cxx_name = "anchorRow"]
        fn anchor_row(self: &PhotoGrid) -> i32;

        /// A rubber band starts: what is selected now is kept when `additive` (Ctrl).
        #[qinvokable]
        #[cxx_name = "rubberBegin"]
        fn rubber_begin(self: Pin<&mut PhotoGrid>, additive: bool);

        /// The rubber band covers these rows and columns of the grid (`columns` wide): they are selected,
        /// besides what `rubberBegin` kept.
        #[qinvokable]
        #[cxx_name = "rubberTo"]
        fn rubber_to(
            self: Pin<&mut PhotoGrid>,
            first_row: i32,
            last_row: i32,
            first_column: i32,
            last_column: i32,
            columns: i32,
        );

        /// The rubber band is over.
        #[qinvokable]
        #[cxx_name = "rubberEnd"]
        fn rubber_end(self: Pin<&mut PhotoGrid>);

        /// Rates every selected photo (0 clears) as one action, one step of the history; how many were
        /// rated.
        #[qinvokable]
        #[cxx_name = "rateSelection"]
        fn rate_selection(self: Pin<&mut PhotoGrid>, rating: i32) -> i32;

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
        type KeywordList = super::KeywordListRust;

        /// Reads the vocabulary and how many photos carry each keyword again.
        #[qinvokable]
        fn refresh(self: Pin<&mut KeywordList>);

        /// Shows the keywords whose name contains `text` (any case) with their ancestors; nothing typed
        /// shows the whole tree, with what was collapsed.
        #[qinvokable]
        #[cxx_name = "setFilter"]
        fn set_filter(self: Pin<&mut KeywordList>, text: &QString);

        /// Collapses or expands the keyword in `row`.
        #[qinvokable]
        #[cxx_name = "toggleExpanded"]
        fn toggle_expanded(self: Pin<&mut KeywordList>, row: i32);

        /// Tells which keywords the selection carries (`KeywordUsage` JSON) and how many photos it has.
        #[qinvokable]
        #[cxx_name = "applyUsage"]
        fn apply_usage(self: Pin<&mut KeywordList>, usage: &QString, selected: i32);

        /// The keyword in `row` (its identifier) and its name.
        #[qinvokable]
        #[cxx_name = "idAt"]
        fn id_at(self: &KeywordList, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "nameAt"]
        fn name_at(self: &KeywordList, row: i32) -> QString;

        /// The row of the best match for what was typed (the name itself, else the first that starts with
        /// it, else the first that contains it), -1 when there is none.
        #[qinvokable]
        #[cxx_name = "bestMatch"]
        fn best_match(self: &KeywordList, text: &QString) -> i32;

        /// Adds `name` to the vocabulary under the keyword `parent` (an identifier; empty for the top
        /// level); its identifier, or the existing keyword's when that name is there already, or `error:`
        /// and why not.
        #[qinvokable]
        fn create(self: Pin<&mut KeywordList>, name: &QString, parent: &QString) -> QString;

        /// Renames the keyword in `row`; empty, or why not.
        #[qinvokable]
        fn rename(self: Pin<&mut KeywordList>, row: i32, name: &QString) -> QString;

        /// The identifier of the keyword named `name` (any case) under `parent` (empty for the top level),
        /// or empty when there is none.
        #[qinvokable]
        #[cxx_name = "findSibling"]
        fn find_sibling(self: &KeywordList, name: &QString, parent: &QString) -> QString;

        /// The name of the keyword `id`, or empty when it is not in the vocabulary.
        #[qinvokable]
        #[cxx_name = "nameOf"]
        fn name_of(self: &KeywordList, id: &QString) -> QString;

        /// Whether the keyword `id` is still in the vocabulary.
        #[qinvokable]
        #[cxx_name = "hasKeyword"]
        fn has_keyword(self: &KeywordList, id: &QString) -> bool;

        /// Whether the keyword `id` can be put under `parent` (empty for the top level).
        #[qinvokable]
        #[cxx_name = "canMove"]
        fn can_move(self: &KeywordList, id: &QString, parent: &QString) -> bool;

        /// Where `id` can go (JSON `[{"id", "path"}]`), for the Move dialog.
        #[qinvokable]
        #[cxx_name = "moveTargets"]
        fn move_targets(self: &KeywordList, id: &QString) -> QString;

        /// What deleting `id` takes with it (JSON `{"name", "keywords", "photos"}`).
        #[qinvokable]
        fn branch(self: &KeywordList, id: &QString) -> QString;

        /// Puts `id` under `parent` (empty for the top level); empty, or why not.
        #[qinvokable]
        #[cxx_name = "moveKeyword"]
        fn move_keyword(self: Pin<&mut KeywordList>, id: &QString, parent: &QString) -> QString;

        /// Deletes `id` and its branch; empty, or why not.
        #[qinvokable]
        fn remove(self: Pin<&mut KeywordList>, id: &QString) -> QString;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut KeywordList>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut KeywordList>);
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut KeywordList>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
        #[inherit]
        fn index(self: &KeywordList, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &KeywordList, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &KeywordList) -> QHash_i32_QByteArray;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &KeywordList, _parent: &QModelIndex) -> i32;
    }

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

use auroraw_catalogue::{Cursor, Filter, FlagFilter};
use auroraw_engine::{Command, Engine, KnownWorkspace};
use auroraw_types::PhotoId;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};

use crate::keyword_list::KeywordListRust;
use crate::selection::Selection;
use crate::session;
use crate::source_list::SourceListRust;

/// Qt::UserRole and the next one.
const ROLE_PHOTO_ID: i32 = 0x0100;
const ROLE_RATING: i32 = 0x0101;
const ROLE_SELECTED: i32 = 0x0102;
const ROLE_FLAG: i32 = 0x0103;

struct Item {
    id: PhotoId,
    rating: u8,
    /// The effective flag: 0 none, 1 picked, 2 rejected.
    flag: u8,
}

/// What this grid asked the engine for and has not seen it confirm: until the catalogue says the same (or
/// a moment passes), what it says is older than what was asked, and not to be shown.
struct Pending {
    rating: Option<u8>,
    flag: Option<u8>,
    at: Instant,
}

/// The Rust side of the grid model.
#[derive(Default)]
pub struct PhotoGridRust {
    count: i32,
    min_rating: i32,
    flag_filter: i32,
    keyword_filter: QString,
    items: Vec<Item>,
    /// Where each listed photo is.
    rows: HashMap<PhotoId, usize>,
    /// The ratings and flags asked for and not yet confirmed (a quick series of keys must not flicker back).
    pending: HashMap<PhotoId, Pending>,
    selected_count: i32,
    /// What was selected when a rubber band started that adds to it.
    rubber_base: Option<std::collections::HashSet<PhotoId>>,
    /// The photos selected, by identifier, and where ranges start (D-097).
    selection: Selection,
}

/// How long an unconfirmed rating is trusted over the catalogue (a command the engine refused).
const PENDING_FOR: Duration = Duration::from_secs(2);

/// Every photo the catalogue lists in the grid's own order (spike 3: the whole ordered list is cheap
/// even at 100,000 photos; only thumbnails are lazy), rated `min_rating` or more.
fn load_items(filter: &Filter) -> Vec<Item> {
    let Some(session) = session::current() else {
        return Vec::new();
    };
    let Ok(catalogue) = session.engine.read_catalogue() else {
        return Vec::new();
    };
    let mut items = Vec::new();
    let mut after = None;
    loop {
        let Ok(page) = catalogue.list_filtered(filter, after, 5000) else {
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
            flag: row.effective_flag,
        }));
    }
    items
}

impl qobject::PhotoGrid {
    pub fn load(mut self: Pin<&mut Self>) {
        let mut items = load_items(&self.filter());
        // A rating or a flag asked for a moment ago may not be in the catalogue yet: it stays what was asked.
        self.as_mut()
            .rust_mut()
            .pending
            .retain(|_, pending| pending.at.elapsed() < PENDING_FOR);
        for item in &mut items {
            if let Some(pending) = self.pending.get(&item.id) {
                item.rating = pending.rating.unwrap_or(item.rating);
                item.flag = pending.flag.unwrap_or(item.flag);
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
        self.as_mut().set_count(count);
        // What was selected and is still listed stays selected.
        let listed = self.ids();
        self.as_mut().rust_mut().selection.retain(&listed);
        let selected = self.selection.len() as i32;
        self.set_selected_count(selected);
    }

    pub fn filter_by(mut self: Pin<&mut Self>, min_rating: i32) {
        self.as_mut().set_min_rating(min_rating.clamp(0, 5));
        // A new filter is a new list: nothing of the old one stays selected.
        self.as_mut().rust_mut().selection.none();
        self.load();
    }

    /// What the grid lists now: the rating, the flags and the keyword filters together.
    fn filter(&self) -> Filter {
        Filter {
            min_rating: self.min_rating.clamp(0, 5) as u8,
            flags: match self.flag_filter {
                1 => FlagFilter::All,
                2 => FlagFilter::Picked,
                3 => FlagFilter::Rejected,
                _ => FlagFilter::NotRejected,
            },
            keyword: auroraw_types::KeywordId::from_str(&self.keyword_filter.to_string()).ok(),
        }
    }

    pub fn filter_flags(mut self: Pin<&mut Self>, flags: i32) {
        self.as_mut().set_flag_filter(flags.clamp(0, 3));
        self.as_mut().rust_mut().selection.none();
        self.load();
    }

    pub fn filter_keyword(mut self: Pin<&mut Self>, keyword: &QString) {
        self.as_mut().set_keyword_filter(keyword.clone());
        self.as_mut().rust_mut().selection.none();
        self.load();
    }

    fn ids(&self) -> Vec<PhotoId> {
        self.items.iter().map(|item| item.id).collect()
    }

    fn id_of(&self, row: i32) -> Option<PhotoId> {
        usize::try_from(row)
            .ok()
            .and_then(|row| self.items.get(row))
            .map(|item| item.id)
    }

    /// Tells the views that the selection changed: every row's `selected` (only the ones on screen cost).
    fn selection_changed(mut self: Pin<&mut Self>) {
        let last = self.items.len() as i32 - 1;
        if last >= 0 {
            let (first, end) = (
                self.index(0, 0, &QModelIndex::default()),
                self.index(last, 0, &QModelIndex::default()),
            );
            let mut roles = QVector::<i32>::default();
            roles.append(ROLE_SELECTED);
            self.as_mut().data_changed(&first, &end, &roles);
        }
        let selected = self.selection.len() as i32;
        self.set_selected_count(selected);
    }

    pub fn select_only(mut self: Pin<&mut Self>, row: i32) {
        if let Some(id) = self.id_of(row) {
            self.as_mut().rust_mut().selection.only(id);
            self.selection_changed();
        }
    }

    pub fn toggle(mut self: Pin<&mut Self>, row: i32) {
        if let Some(id) = self.id_of(row) {
            self.as_mut().rust_mut().selection.toggle(id);
            self.selection_changed();
        }
    }

    pub fn extend_to(mut self: Pin<&mut Self>, row: i32, additive: bool) {
        let Some(target) = usize::try_from(row).ok().filter(|r| *r < self.items.len()) else {
            return;
        };
        let anchor = self
            .selection
            .anchor()
            .and_then(|id| self.rows.get(&id).copied())
            .unwrap_or(target);
        let ids = self.ids();
        self.as_mut()
            .rust_mut()
            .selection
            .range(&ids, anchor, target, additive);
        self.selection_changed();
    }

    pub fn select_all(mut self: Pin<&mut Self>) {
        let ids = self.ids();
        self.as_mut().rust_mut().selection.all(&ids);
        self.selection_changed();
    }

    pub fn select_none(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().selection.none();
        self.selection_changed();
    }

    pub fn invert(mut self: Pin<&mut Self>) {
        let ids = self.ids();
        self.as_mut().rust_mut().selection.invert(&ids);
        self.selection_changed();
    }

    pub fn select_photos(mut self: Pin<&mut Self>, ids: &QString) {
        let listed: Vec<PhotoId> = ids
            .to_string()
            .split(',')
            .filter_map(|text| PhotoId::from_str(text).ok())
            .filter(|id| self.rows.contains_key(id))
            .collect();
        self.as_mut().rust_mut().selection.set(listed);
        self.selection_changed();
    }

    pub fn is_selected(&self, row: i32) -> bool {
        self.id_of(row)
            .is_some_and(|id| self.selection.contains(&id))
    }

    pub fn first_selected_row(&self) -> i32 {
        self.items
            .iter()
            .position(|item| self.selection.contains(&item.id))
            .map_or(-1, |row| row as i32)
    }

    pub fn anchor_row(&self) -> i32 {
        self.selection
            .anchor()
            .and_then(|id| self.rows.get(&id).copied())
            .map_or(-1, |row| row as i32)
    }

    /// The selected photos in row order.
    fn selected_rows(&self) -> Vec<usize> {
        (0..self.items.len())
            .filter(|row| self.selection.contains(&self.items[*row].id))
            .collect()
    }

    pub fn flag_selection(mut self: Pin<&mut Self>, kind: &QString) -> i32 {
        use auroraw_engine::Flag;
        let Some(session) = session::current() else {
            return 0;
        };
        let target: u8 = match kind.to_string().as_str() {
            "pick" => 1,
            "reject" => 2,
            _ => 0,
        };
        let rows = self.selected_rows();
        if rows.is_empty() {
            return 0;
        }
        // The same key again takes the flag off: when every selected photo has it already.
        let all_have = rows.iter().all(|row| self.items[*row].flag == target);
        let new = if target != 0 && all_have { 0 } else { target };
        let flag = match new {
            1 => Some(Flag::Picked),
            2 => Some(Flag::Rejected),
            _ => None,
        };
        let mut commands: Vec<Command> = rows
            .iter()
            .map(|row| Command::SetFlag {
                photo_id: self.items[*row].id,
                flag,
            })
            .collect();
        let command = if commands.len() == 1 {
            commands.remove(0)
        } else {
            Command::Batch { commands }
        };
        let _ = session.engine.submit(command);
        // The cells show it at once (a rejected one stays, dimmed, until the list is read again).
        for row in &rows {
            let id = self.items[*row].id;
            self.as_mut().rust_mut().items[*row].flag = new;
            self.as_mut().ask(id, None, Some(new));
        }
        self.as_mut().redraw_all();
        rows.len() as i32
    }

    pub fn keyword_usage(&self) -> QString {
        let Some(session) = session::current() else {
            return QString::from("{}");
        };
        let selected: Vec<PhotoId> = self
            .selected_rows()
            .iter()
            .map(|row| self.items[*row].id)
            .collect();
        let usage = session
            .engine
            .read_catalogue()
            .ok()
            .and_then(|catalogue| catalogue.keyword_usage(&selected).ok())
            .unwrap_or_default();
        let map: serde_json::Map<String, serde_json::Value> = usage
            .into_iter()
            .map(|(id, count)| (id.to_string(), serde_json::Value::from(count)))
            .collect();
        QString::from(serde_json::Value::Object(map).to_string().as_str())
    }

    pub fn keyword_selection(self: Pin<&mut Self>, keyword: &QString, add: bool) -> i32 {
        let Some(session) = session::current() else {
            return 0;
        };
        let Ok(keyword_id) = auroraw_types::KeywordId::from_str(&keyword.to_string()) else {
            return 0;
        };
        let rows = self.selected_rows();
        if rows.is_empty() {
            return 0;
        }
        let mut commands: Vec<Command> = rows
            .iter()
            .map(|row| {
                let photo_id = self.items[*row].id;
                if add {
                    Command::AddKeyword {
                        photo_id,
                        keyword_id,
                    }
                } else {
                    Command::RemoveKeyword {
                        photo_id,
                        keyword_id,
                    }
                }
            })
            .collect();
        let command = if commands.len() == 1 {
            commands.remove(0)
        } else {
            Command::Batch { commands }
        };
        let _ = session.engine.submit(command);
        rows.len() as i32
    }

    pub fn create_keyword_selection(
        self: Pin<&mut Self>,
        name: &QString,
        parent: &QString,
    ) -> QString {
        let Some(session) = session::current() else {
            return QString::from("error:other:No workspace is open.");
        };
        let rows = self.selected_rows();
        let keyword_id = auroraw_types::KeywordId::random();
        let parent = auroraw_types::KeywordId::from_str(&parent.to_string()).ok();
        let mut commands = vec![Command::CreateKeyword {
            name: name.to_string(),
            parent,
            id: Some(keyword_id),
        }];
        commands.extend(rows.iter().map(|row| Command::AddKeyword {
            photo_id: self.items[*row].id,
            keyword_id,
        }));
        let command = if commands.len() == 1 {
            commands.remove(0)
        } else {
            Command::Batch { commands }
        };
        match session.engine.submit_and_wait(command) {
            Ok(_) => QString::from(keyword_id.to_string().as_str()),
            Err(e) => QString::from(format!("error:{}", crate::keyword_list::reason(&e)).as_str()),
        }
    }

    /// Redraws every cell's rating and flag.
    fn redraw_all(mut self: Pin<&mut Self>) {
        let last = self.items.len() as i32 - 1;
        if last < 0 {
            return;
        }
        let (first, end) = (
            self.index(0, 0, &QModelIndex::default()),
            self.index(last, 0, &QModelIndex::default()),
        );
        let mut roles = QVector::<i32>::default();
        roles.append(ROLE_RATING);
        roles.append(ROLE_FLAG);
        self.as_mut().data_changed(&first, &end, &roles);
    }

    pub fn rubber_begin(mut self: Pin<&mut Self>, additive: bool) {
        let base = if additive {
            self.selection.snapshot()
        } else {
            Default::default()
        };
        self.as_mut().rust_mut().rubber_base = Some(base);
    }

    pub fn rubber_to(
        mut self: Pin<&mut Self>,
        first_row: i32,
        last_row: i32,
        first_column: i32,
        last_column: i32,
        columns: i32,
    ) {
        let Some(base) = self.rubber_base.clone() else {
            return;
        };
        let columns = columns.max(1);
        let mut covered = Vec::new();
        for row in first_row.max(0)..=last_row {
            for column in first_column.max(0)..=last_column.min(columns - 1) {
                if let Some(item) = self.items.get((row * columns + column) as usize) {
                    covered.push(item.id);
                }
            }
        }
        self.as_mut().rust_mut().selection.set_over(&base, covered);
        self.selection_changed();
    }

    pub fn rubber_end(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().rubber_base = None;
    }

    pub fn rate_selection(mut self: Pin<&mut Self>, rating: i32) -> i32 {
        let Some(session) = session::current() else {
            return 0;
        };
        let rating = rating.clamp(0, 5) as u8;
        let rows: Vec<usize> = (0..self.items.len())
            .filter(|row| self.selection.contains(&self.items[*row].id))
            .collect();
        if rows.is_empty() {
            return 0;
        }
        let mut commands: Vec<Command> = rows
            .iter()
            .map(|row| Command::SetRating {
                photo_id: self.items[*row].id,
                rating,
            })
            .collect();
        // One photo is a plain edit; several are one action, one step of the history (D-096).
        let command = if commands.len() == 1 {
            commands.remove(0)
        } else {
            Command::Batch { commands }
        };
        let _ = session.engine.submit(command);
        // The cells show the new rating at once; the engine's own events confirm it.
        for row in &rows {
            let id = self.items[*row].id;
            self.as_mut().rust_mut().items[*row].rating = rating;
            self.as_mut().ask(id, Some(rating), None);
        }
        self.as_mut().redraw_all();
        rows.len() as i32
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
        if let Some(pending) = self.pending.get(&id) {
            let fresh = pending.at.elapsed() < PENDING_FOR;
            let stale = pending.rating.is_some_and(|r| r != photo.effective_rating)
                || pending.flag.is_some_and(|f| f != photo.effective_flag);
            if fresh && stale {
                // An older state of a quick series of keys: the last one's own event follows.
                return;
            }
            self.as_mut().rust_mut().pending.remove(&id);
        }
        if self.items[row].rating != photo.effective_rating
            || self.items[row].flag != photo.effective_flag
        {
            self.as_mut().rust_mut().items[row].rating = photo.effective_rating;
            self.as_mut().rust_mut().items[row].flag = photo.effective_flag;
            self.redraw_photo(row);
        }
    }

    /// Redraws a cell's rating and flag.
    fn redraw_photo(mut self: Pin<&mut Self>, row: usize) {
        let index = self.index(row as i32, 0, &QModelIndex::default());
        let mut roles = QVector::<i32>::default();
        roles.append(ROLE_RATING);
        roles.append(ROLE_FLAG);
        self.as_mut().data_changed(&index, &index, &roles);
    }

    /// Remembers what was asked of the engine for a photo, until the catalogue says the same.
    fn ask(mut self: Pin<&mut Self>, id: PhotoId, rating: Option<u8>, flag: Option<u8>) {
        let (before_rating, before_flag) = self
            .pending
            .get(&id)
            .map_or((None, None), |p| (p.rating, p.flag));
        self.as_mut().rust_mut().pending.insert(
            id,
            Pending {
                rating: rating.or(before_rating),
                flag: flag.or(before_flag),
                at: Instant::now(),
            },
        );
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
        let flag = match photo.effective_flag {
            1 => Some("\u{2714}".to_string()),
            2 => Some("\u{2716}".to_string()),
            _ => None,
        };
        let parts: Vec<String> = [Some(photo.filename), photo.camera, Some(stars), flag]
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
        self.as_mut().ask(id, Some(rating), None);
        self.redraw_photo(row as usize);
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(item) = self.items.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_PHOTO_ID => QVariant::from(&QString::from(item.id.to_string().as_str())),
            ROLE_RATING => QVariant::from(&i32::from(item.rating)),
            ROLE_SELECTED => QVariant::from(&self.selection.contains(&item.id)),
            ROLE_FLAG => QVariant::from(&i32::from(item.flag)),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_PHOTO_ID, QByteArray::from("photoId"));
        roles.insert(ROLE_RATING, QByteArray::from("rating"));
        roles.insert(ROLE_SELECTED, QByteArray::from("selected"));
        roles.insert(ROLE_FLAG, QByteArray::from("flag"));
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
