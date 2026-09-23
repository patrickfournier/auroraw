// SPDX-License-Identifier: GPL-3.0-or-later
//! The Slint user interface. It talks only to the engine (architecture §3.2): the types this
//! crate uses beyond `auroraw_engine` itself are plain data (`PhotoId`, `Thumbnail`) the engine's
//! own public API already hands back, the same way `cli` uses `auroraw_format`'s and
//! `auroraw_types`'s data types directly without depending on how the engine computes them.
//! Work package WP8: the shell and the library grid. Grows with later work packages (cull mode,
//! develop, publish, WP9 onward).

pub mod commands;
mod grid;
#[cfg(test)]
mod i18n_check;

// Slint's own generated code carries no doc comments; this crate's `missing_docs` lint (workspace
// wide) would otherwise warn on every item the macro below produces.
#[allow(missing_docs)]
mod generated {
    slint::include_modules!();
}
use generated::{Cell, GridRow, MainWindow};

use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use auroraw_engine::{Command, Engine, Event, EventReceiver};
use auroraw_types::PhotoId;
use grid::{GridState, Item, RowModel};
use slint::{ComponentHandle, Timer, TimerMode};

/// Every photo the catalogue currently has, in the grid's own order (spike 3: the whole ordered
/// list is cheap even at 100,000 photos; only thumbnails are lazy). `min_rating` mirrors the
/// filter bar's own buttons (0 for no filter).
fn load_items(engine: &Engine, min_rating: u8) -> auroraw_engine::Result<Vec<Item>> {
    let catalogue = engine.read_catalogue()?;
    let mut items = Vec::new();
    let mut after = None;
    loop {
        let page = if min_rating == 0 {
            catalogue.list_recent(after, 5000)?
        } else {
            catalogue.list_by_min_rating(min_rating, after, 5000)?
        };
        if page.is_empty() {
            break;
        }
        after = page.last().map(|row| auroraw_catalogue::Cursor {
            capture_time: row.capture_time,
            id: row.id,
        });
        items.extend(page.into_iter().map(|row| Item {
            id: row.id,
            rating: row.effective_rating,
        }));
    }
    Ok(items)
}

fn photo_row(engine: &Engine, id: PhotoId) -> Option<auroraw_catalogue::PhotoRow> {
    engine.read_catalogue().ok()?.photo(&id).ok()?
}

fn summary_of(engine: &Engine, id: PhotoId) -> String {
    match photo_row(engine, id) {
        Some(row) => {
            let stars = if row.rating > 0 {
                "★".repeat(row.rating as usize)
            } else {
                String::new()
            };
            format!(
                "{} — {} — {stars}",
                row.filename,
                row.camera.unwrap_or_default()
            )
        }
        _ => String::new(),
    }
}

/// Runs the shell until its window is closed. `previews_path` is where the thumbnail cache lives
/// (D-075); this crate resolves no cache directory itself, matching every other crate under
/// `engine`'s own precedent -- the caller (`app`) decides.
pub fn run(
    engine: Engine,
    events: EventReceiver,
    previews_path: &Path,
) -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    let state = Rc::new(GridState::default());
    let items = load_items(&engine, 0).unwrap_or_default();
    ui.set_status(format!("{} photos", items.len()).into());
    state.set_items(items);

    let thumbnails = Rc::new(
        engine
            .start_thumbnails(previews_path, 4)
            .expect("the previews database can be opened"),
    );
    ui.set_rows(slint::ModelRc::new(RowModel::new(
        state.clone(),
        thumbnails.clone(),
    )));

    // Thumbnails and engine events both arrive off the UI thread; a short repeating timer drains
    // each, the same shape spike 3 used for pictures arriving from its own worker threads.
    let thumbnail_timer = Timer::default();
    {
        let (state, thumbnails) = (state.clone(), thumbnails.clone());
        thumbnail_timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            for (id, thumbnail) in thumbnails.poll() {
                state.deliver(id, &thumbnail);
            }
        });
    }
    let event_timer = Timer::default();
    {
        let (state, engine, weak) = (state.clone(), engine.clone(), ui.as_weak());
        event_timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
            while let Some(event) = events.try_recv() {
                if let Event::PhotoChanged(id) = event
                    && let Some(row) = photo_row(&engine, id)
                {
                    state.set_rating(id, row.effective_rating);
                    if state.selected_id() == Some(id)
                        && let Some(ui) = weak.upgrade()
                    {
                        ui.set_selected_summary(summary_of(&engine, id).into());
                    }
                }
            }
        });
    }

    {
        let (state, engine, weak) = (state.clone(), engine.clone(), ui.as_weak());
        ui.on_cell_clicked(move |flat_index| {
            let Some(ui) = weak.upgrade() else { return };
            if let Some(id) = state.select(Some(flat_index as usize)) {
                ui.set_selected_summary(summary_of(&engine, id).into());
            }
        });
    }

    {
        let (state, engine, weak) = (state.clone(), engine.clone(), ui.as_weak());
        ui.on_rate_selected(move |digit| {
            let Some(rating) = digit.parse::<u8>().ok().filter(|r| *r <= 5) else {
                return;
            };
            let Some(id) = state.selected_id() else {
                return;
            };
            let _ = engine.submit(Command::SetRating {
                photo_id: id,
                rating,
            });
            state.set_rating(id, rating);
            if let Some(ui) = weak.upgrade() {
                ui.set_selected_summary(summary_of(&engine, id).into());
            }
        });
    }

    {
        let (state, engine, weak) = (state.clone(), engine.clone(), ui.as_weak());
        ui.on_move_selection(move |dx, dy| {
            let Some(ui) = weak.upgrade() else { return };
            if state.len() == 0 {
                return;
            }
            let next = next_index(&state, dx, dy, grid::COLS as i32);
            if let Some(id) = state.select(Some(next)) {
                ui.set_selected_summary(summary_of(&engine, id).into());
            }
        });
    }

    {
        let (engine, state, weak) = (engine.clone(), state.clone(), ui.as_weak());
        ui.on_set_filter(move |min_rating| {
            let Some(ui) = weak.upgrade() else { return };
            let items = load_items(&engine, min_rating as u8).unwrap_or_default();
            ui.set_status(format!("{} photos", items.len()).into());
            state.set_items(items);
            ui.set_selected_summary(String::new().into());
        });
    }

    ui.run()
}

/// The flat index arrow navigation lands on, clamped to the list's bounds.
fn next_index(state: &GridState, dx: i32, dy: i32, cols: i32) -> usize {
    let len = state.len() as i32;
    let current = state.current_selected_or_zero() as i32;
    (current + dx + dy * cols).clamp(0, (len - 1).max(0)) as usize
}
