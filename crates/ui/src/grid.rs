// SPDX-License-Identifier: GPL-3.0-or-later
//! The library grid's model (spec §3, §4; M1 plan WP8, spike 3's own shape): a `slint::Model`
//! whose rows are built on demand from a plain, already-fetched list of photos, asking
//! `auroraw_engine::ThumbnailService` for whatever cell has no thumbnail yet and drawing an empty
//! cell only until one arrives (testing strategy §6: "the model never returns an empty cell for a
//! row that is on screen", checked without a window in this module's own tests).

use std::cell::RefCell;
use std::collections::HashMap;

use auroraw_engine::ThumbnailService;
use auroraw_imaging::Thumbnail;
use auroraw_types::PhotoId;
use slint::{
    Image, Model, ModelNotify, ModelRc, ModelTracker, Rgba8Pixel, SharedPixelBuffer, VecModel,
};

use crate::{Cell, GridRow};

/// The columns a row holds (spike 3's own number, measured against a typical window width).
pub const COLS: usize = 8;

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
    selected: RefCell<Option<usize>>,
    notify: ModelNotify,
}

impl Default for GridState {
    fn default() -> Self {
        Self {
            items: RefCell::new(Vec::new()),
            index: RefCell::new(HashMap::new()),
            cache: RefCell::new(HashMap::new()),
            selected: RefCell::new(None),
            notify: ModelNotify::default(),
        }
    }
}

impl GridState {
    /// Replaces the whole list (a fresh filter or the first load) and clears whatever thumbnails
    /// were cached for photos no longer shown, so the cache cannot grow without bound across
    /// many filter changes.
    pub fn set_items(&self, items: Vec<Item>) {
        *self.index.borrow_mut() = items.iter().enumerate().map(|(i, it)| (it.id, i)).collect();
        let keep: std::collections::HashSet<PhotoId> = items.iter().map(|it| it.id).collect();
        self.cache.borrow_mut().retain(|id, _| keep.contains(id));
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
            i / COLS
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
        let row = self.index.borrow().get(&id).copied()? / COLS;
        self.notify.row_changed(row);
        Some(row)
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
            self.notify.row_changed(old / COLS);
        }
        *self.selected.borrow_mut() = index;
        if let Some(index) = index {
            self.notify.row_changed(index / COLS);
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
        self.state.len().div_ceil(COLS)
    }

    fn row_data(&self, row: usize) -> Option<Self::Data> {
        let items = self.state.items.borrow();
        let selected = *self.state.selected.borrow();
        let cache = self.state.cache.borrow();
        let cells: Vec<Cell> = (0..COLS)
            .filter_map(|c| items.get(row * COLS + c).map(|it| (row * COLS + c, it)))
            .map(|(flat, it)| match cache.get(&it.id) {
                Some(image) => Cell {
                    photo_id: it.id.to_string().into(),
                    thumb: image.clone(),
                    rating: it.rating as i32,
                    ready: true,
                    selected: selected == Some(flat),
                },
                None => {
                    self.thumbnails.request(it.id);
                    Cell {
                        photo_id: it.id.to_string().into(),
                        thumb: Image::default(),
                        rating: it.rating as i32,
                        ready: false,
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
            COLS,
            "every column of a full row is present"
        );
        for i in 0..COLS {
            let cell = row.cells.row_data(i).unwrap();
            assert!(
                !cell.photo_id.is_empty(),
                "a cell always has an identifier, ready or not"
            );
        }
    }

    #[test]
    fn a_short_last_row_has_only_as_many_cells_as_photos_remain() {
        let state = state_with(COLS + 3);
        let model = RowModel::new(state, service());
        assert_eq!(model.row_count(), 2);
        assert_eq!(model.row_data(1).unwrap().cells.row_count(), 3);
    }

    #[test]
    fn selecting_a_cell_marks_only_that_cell_selected() {
        let state = state_with(COLS * 2);
        *state.selected.borrow_mut() = Some(COLS + 2);
        let model = RowModel::new(state, service());
        let row = model.row_data(1).unwrap();
        for i in 0..COLS {
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
}
