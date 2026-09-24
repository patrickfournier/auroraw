// SPDX-License-Identifier: GPL-3.0-or-later
//! The library grid's model (spec §3, §4; M1 plan WP8, spike 3's own shape): a `slint::Model`
//! whose rows are built on demand from a plain, already-fetched list of photos, asking
//! `auroraw_engine::ThumbnailService` for whatever cell has no thumbnail yet and drawing an empty
//! cell only until one arrives (testing strategy §6: "the model never returns an empty cell for a
//! row that is on screen", checked without a window in this module's own tests).

use std::cell::{Cell as Counter, RefCell};
use std::collections::{HashMap, HashSet};

use auroraw_engine::ThumbnailService;
use auroraw_imaging::Thumbnail;
use auroraw_types::PhotoId;
use slint::{
    Image, Model, ModelNotify, ModelRc, ModelTracker, Rgba8Pixel, SharedPixelBuffer, VecModel,
};

use crate::generated::{Cell, GridRow};

/// The columns a row holds until the window says how many fit (spike 3's own number, measured
/// against a typical window width).
pub const DEFAULT_COLS: usize = 8;

/// Where a keyboard move goes, beyond a step to a neighbour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// One screenful up, in the same column.
    PageUp,
    /// One screenful down, in the same column.
    PageDown,
    /// The first photo.
    Home,
    /// The last photo.
    End,
}

/// The index a step of `(dx, dy)` cells from `current` lands on, in a grid `cols` wide with `len`
/// photos: always a photo, never outside the list.
pub fn step(current: usize, dx: i32, dy: i32, cols: usize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let target = current as i64 + i64::from(dx) + i64::from(dy) * cols as i64;
    target.clamp(0, len as i64 - 1) as usize
}

/// The index a [`Jump`] lands on: a page moves by the rows that fit on screen (at least one), keeps
/// its column, and stops at the first or last photo (the last row may be shorter than the column,
/// so a page down onto it lands on its last photo).
pub fn jump(kind: Jump, current: usize, cols: usize, visible_rows: usize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let page = visible_rows.max(1) * cols;
    match kind {
        Jump::Home => 0,
        Jump::End => len - 1,
        Jump::PageUp => current.saturating_sub(page),
        Jump::PageDown => (current + page).min(len - 1),
    }
}

/// One photo, as much as the grid needs to draw a cell and act on a click.
#[derive(Clone, Copy)]
pub struct Item {
    pub id: PhotoId,
    pub rating: u8,
}

fn to_slint_image(thumbnail: &Thumbnail) -> Image {
    // The previews database stores JPEG bytes (D-075); decoding them here, once per cell, is the
    // same cost spike 3's own worker threads paid, just moved to whichever thread calls this
    // (always the UI thread, since it only runs once per newly arrived thumbnail, not per frame).
    match image::load_from_memory_with_format(&thumbnail.jpeg, image::ImageFormat::Jpeg) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
            ))
        }
        Err(_) => Image::default(),
    }
}

/// The grid's state: the current (already filtered and ordered) list of photos, a bounded cache
/// of decoded thumbnails, and which cell is selected.
pub struct GridState {
    items: RefCell<Vec<Item>>,
    index: RefCell<HashMap<PhotoId, usize>>,
    cache: RefCell<HashMap<PhotoId, Image>>,
    unavailable: RefCell<HashSet<PhotoId>>,
    selected: RefCell<Option<usize>>,
    /// How many cells a row holds: what fits the window's width (see [`GridState::set_columns`]).
    cols: Counter<usize>,
    notify: ModelNotify,
}

impl Default for GridState {
    fn default() -> Self {
        Self {
            items: RefCell::new(Vec::new()),
            index: RefCell::new(HashMap::new()),
            cache: RefCell::new(HashMap::new()),
            unavailable: RefCell::new(HashSet::new()),
            selected: RefCell::new(None),
            cols: Counter::new(DEFAULT_COLS),
            notify: ModelNotify::default(),
        }
    }
}

impl GridState {
    /// The cells a row holds right now.
    pub fn columns(&self) -> usize {
        self.cols.get()
    }

    /// Re-flows the rows to `cols` cells each (the window was resized): every row changes, the
    /// selection stays on the same photo. Does nothing when the count is the same.
    pub fn set_columns(&self, cols: usize) {
        let cols = cols.max(1);
        if self.cols.replace(cols) != cols {
            self.notify.reset();
        }
    }

    /// The row a photo's cell is in.
    pub fn row_of(&self, flat_index: usize) -> usize {
        flat_index / self.columns()
    }

    /// Replaces the whole list (a fresh filter or the first load) and clears whatever thumbnails
    /// were cached for photos no longer shown, so the cache cannot grow without bound across
    /// many filter changes.
    pub fn set_items(&self, items: Vec<Item>) {
        *self.index.borrow_mut() = items.iter().enumerate().map(|(i, it)| (it.id, i)).collect();
        let keep: std::collections::HashSet<PhotoId> = items.iter().map(|it| it.id).collect();
        self.cache.borrow_mut().retain(|id, _| keep.contains(id));
        self.unavailable.borrow_mut().retain(|id| keep.contains(id));
        *self.items.borrow_mut() = items;
        *self.selected.borrow_mut() = None;
        self.notify.reset();
    }

    /// Applies a rating change already made through the engine, so the cell redraws without a
    /// fresh query.
    pub fn set_rating(&self, id: PhotoId, rating: u8) {
        let row = {
            let mut items = self.items.borrow_mut();
            let Some(i) = self.index.borrow().get(&id).copied() else {
                return;
            };
            items[i].rating = rating;
            i / self.columns()
        };
        self.notify.row_changed(row);
    }

    /// Stores a thumbnail that just arrived and reports which row needs to redraw, if the photo
    /// is still in the current list (it may have scrolled out of a filter that changed while the
    /// request was in flight).
    pub fn deliver(&self, id: PhotoId, thumbnail: &Thumbnail) -> Option<usize> {
        self.cache
            .borrow_mut()
            .insert(id, to_slint_image(thumbnail));
        let row = self.index.borrow().get(&id).copied()? / self.columns();
        self.notify.row_changed(row);
        Some(row)
    }

    /// Records that no thumbnail can be made for `id` (an unreadable file, or a RAW with no
    /// embedded preview `rawler` can decode): its cell says so instead of staying empty, and is
    /// never asked for again, which would otherwise repeat on every redraw.
    pub fn mark_unavailable(&self, id: PhotoId) {
        self.unavailable.borrow_mut().insert(id);
        if let Some(i) = self.index.borrow().get(&id).copied() {
            self.notify.row_changed(i / self.columns());
        }
    }

    /// The identifier at a flat cell index, for a click or a keyboard move.
    pub fn id_at(&self, flat_index: usize) -> Option<PhotoId> {
        self.items.borrow().get(flat_index).map(|it| it.id)
    }

    pub fn len(&self) -> usize {
        self.items.borrow().len()
    }

    /// The currently selected photo, if any and if it is still in the list.
    pub fn selected_id(&self) -> Option<PhotoId> {
        let index = (*self.selected.borrow())?;
        self.id_at(index)
    }

    /// The selected flat index, if any.
    pub fn selected_index(&self) -> Option<usize> {
        *self.selected.borrow()
    }

    /// The selected flat index, or 0 (the first cell) if nothing is selected yet: what arrow
    /// navigation starts from.
    pub fn current_selected_or_zero(&self) -> usize {
        self.selected.borrow().unwrap_or(0)
    }

    /// Selects `index` (clamped to the list, `None` if the list is empty), notifying the row
    /// that lost the selection and the row that gained it so both redraw. Returns the newly
    /// selected photo, if there is one.
    pub fn select(&self, index: Option<usize>) -> Option<PhotoId> {
        let len = self.len();
        let index = index.filter(|_| len > 0).map(|i| i.min(len - 1));
        let old = self.selected.borrow_mut().take();
        if let Some(old) = old {
            self.notify.row_changed(old / self.columns());
        }
        *self.selected.borrow_mut() = index;
        if let Some(index) = index {
            self.notify.row_changed(index / self.columns());
        }
        index.and_then(|i| self.id_at(i))
    }
}

/// The `slint::Model` the grid's `rows` property is bound to. Building one row asks the
/// [`ThumbnailService`] for any cell that has no cached image yet; the cell renders empty
/// (`ready: false`) until [`GridState::deliver`] reports it arrived, matching spike 3's own
/// "empty until ready" cell shape.
pub struct RowModel {
    state: std::rc::Rc<GridState>,
    thumbnails: std::rc::Rc<ThumbnailService>,
}

impl RowModel {
    pub fn new(state: std::rc::Rc<GridState>, thumbnails: std::rc::Rc<ThumbnailService>) -> Self {
        Self { state, thumbnails }
    }
}

impl Model for RowModel {
    type Data = GridRow;

    fn row_count(&self) -> usize {
        self.state.len().div_ceil(self.state.columns())
    }

    fn row_data(&self, row: usize) -> Option<Self::Data> {
        let items = self.state.items.borrow();
        let selected = *self.state.selected.borrow();
        let cache = self.state.cache.borrow();
        let unavailable = self.state.unavailable.borrow();
        let cols = self.state.columns();
        let cells: Vec<Cell> = (0..cols)
            .filter_map(|c| items.get(row * cols + c).map(|it| (row * cols + c, it)))
            .map(|(flat, it)| match cache.get(&it.id) {
                Some(image) => Cell {
                    index: flat as i32,
                    photo_id: it.id.to_string().into(),
                    thumb: image.clone(),
                    rating: it.rating as i32,
                    ready: true,
                    unavailable: false,
                    selected: selected == Some(flat),
                },
                None => {
                    let unavailable = unavailable.contains(&it.id);
                    if !unavailable {
                        self.thumbnails.request(it.id);
                    }
                    Cell {
                        index: flat as i32,
                        photo_id: it.id.to_string().into(),
                        thumb: Image::default(),
                        rating: it.rating as i32,
                        ready: false,
                        unavailable,
                        selected: selected == Some(flat),
                    }
                }
            })
            .collect();
        Some(GridRow {
            cells: ModelRc::new(VecModel::from(cells)),
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.state.notify
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn state_with(n: usize) -> std::rc::Rc<GridState> {
        let state = std::rc::Rc::new(GridState::default());
        state.set_items(
            (0..n)
                .map(|_| Item {
                    id: PhotoId::random(),
                    rating: 0,
                })
                .collect(),
        );
        state
    }

    fn service() -> std::rc::Rc<ThumbnailService> {
        let dir = auroraw_testkit::temp_dir();
        let workspace =
            Arc::new(auroraw_workspace::Workspace::create(&dir.path().join("W"), "W").unwrap());
        std::rc::Rc::new(
            ThumbnailService::start(
                workspace,
                dir.path().join("does-not-exist.sqlite"),
                dir.path().join("previews.db"),
                1,
            )
            .unwrap(),
        )
    }

    #[test]
    fn a_row_on_screen_is_never_missing_a_cell_even_before_a_thumbnail_arrives() {
        let state = state_with(16);
        let model = RowModel::new(state.clone(), service());
        let row = model.row_data(0).unwrap();
        assert_eq!(
            row.cells.row_count(),
            DEFAULT_COLS,
            "every column of a full row is present"
        );
        for i in 0..DEFAULT_COLS {
            let cell = row.cells.row_data(i).unwrap();
            assert!(
                !cell.photo_id.is_empty(),
                "a cell always has an identifier, ready or not"
            );
        }
    }

    #[test]
    fn a_short_last_row_has_only_as_many_cells_as_photos_remain() {
        let state = state_with(DEFAULT_COLS + 3);
        let model = RowModel::new(state, service());
        assert_eq!(model.row_count(), 2);
        assert_eq!(model.row_data(1).unwrap().cells.row_count(), 3);
    }

    #[test]
    fn a_photo_no_thumbnail_can_be_made_for_says_so_and_is_not_asked_for_again() {
        let state = state_with(2);
        let id = state.id_at(0).unwrap();
        let model = RowModel::new(state.clone(), service());
        assert!(
            !model
                .row_data(0)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .unavailable
        );
        state.mark_unavailable(id);
        let cell = model.row_data(0).unwrap().cells.row_data(0).unwrap();
        assert!(cell.unavailable && !cell.ready);
        let other = model.row_data(0).unwrap().cells.row_data(1).unwrap();
        assert!(!other.unavailable, "only the photo that failed");
    }

    #[test]
    fn selecting_a_cell_marks_only_that_cell_selected() {
        let state = state_with(DEFAULT_COLS * 2);
        *state.selected.borrow_mut() = Some(DEFAULT_COLS + 2);
        let model = RowModel::new(state, service());
        let row = model.row_data(1).unwrap();
        for i in 0..DEFAULT_COLS {
            let selected = row.cells.row_data(i).unwrap().selected;
            assert_eq!(selected, i == 2, "cell {i}");
        }
    }

    #[test]
    fn set_items_clears_the_selection_and_prunes_the_cache_of_photos_no_longer_listed() {
        let state = GridState::default();
        let a = PhotoId::random();
        state.set_items(vec![Item { id: a, rating: 0 }]);
        *state.selected.borrow_mut() = Some(0);
        state.set_items(vec![Item {
            id: PhotoId::random(),
            rating: 0,
        }]);
        assert!(state.selected.borrow().is_none());
        assert!(state.id_at(0).is_some());
        assert_ne!(state.id_at(0), Some(a));
    }

    #[test]
    fn a_resized_window_re_flows_the_rows_and_keeps_the_selection_on_its_photo() {
        let state = state_with(20);
        *state.selected.borrow_mut() = Some(11);
        let selected = state.selected_id();
        let model = RowModel::new(state.clone(), service());
        assert_eq!(model.row_count(), 3, "8 per row");

        state.set_columns(5);
        assert_eq!(model.row_count(), 4);
        assert_eq!(model.row_data(0).unwrap().cells.row_count(), 5);
        assert_eq!(state.selected_id(), selected);
        assert_eq!(state.row_of(11), 2);
        let cell = model.row_data(2).unwrap().cells.row_data(1).unwrap();
        assert!(cell.selected, "photo 11 is the second cell of row 2 now");

        state.set_columns(0);
        assert_eq!(state.columns(), 1, "never fewer than one column");
    }

    #[test]
    fn steps_stay_inside_the_list_and_move_by_rows_of_the_current_width() {
        assert_eq!(step(10, 1, 0, 8, 20), 11);
        assert_eq!(step(10, 0, 1, 8, 20), 18);
        assert_eq!(step(10, 0, 1, 5, 20), 15);
        assert_eq!(step(2, 0, -1, 8, 20), 0, "clamped at the start");
        assert_eq!(step(19, 1, 0, 8, 20), 19, "clamped at the end");
        assert_eq!(step(0, 0, 1, 8, 0), 0, "an empty list has nothing to go to");
    }

    #[test]
    fn home_end_and_pages_land_on_photos_and_keep_their_column() {
        // 100 photos, 8 per row, 5 rows on screen: a page is 40 photos.
        assert_eq!(jump(Jump::Home, 57, 8, 5, 100), 0);
        assert_eq!(jump(Jump::End, 3, 8, 5, 100), 99);
        assert_eq!(jump(Jump::PageDown, 3, 8, 5, 100), 43);
        assert_eq!(
            jump(Jump::PageDown, 70, 8, 5, 100),
            99,
            "stops at the last photo"
        );
        assert_eq!(jump(Jump::PageUp, 43, 8, 5, 100), 3);
        assert_eq!(
            jump(Jump::PageUp, 10, 8, 5, 100),
            0,
            "stops at the first photo"
        );
        assert_eq!(
            jump(Jump::PageDown, 0, 8, 0, 100),
            8,
            "a page is a row at least"
        );
        assert_eq!(jump(Jump::End, 0, 8, 5, 0), 0, "an empty list");
    }
}
