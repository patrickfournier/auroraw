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
mod headless_tests;
#[cfg(test)]
mod i18n_check;
mod settings;

// Slint's own generated code carries no doc comments; this crate's `missing_docs` lint (workspace
// wide) would otherwise warn on every item the macro below produces.
#[allow(missing_docs)]
mod generated {
    slint::include_modules!();
}
use generated::{Cell, GridRow, MainWindow, Texts, VolumeEntry};

use std::cell::{Cell as StdCell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use auroraw_engine::{Command, Engine, Event, EventReceiver, ImportRequest, JobId};
use auroraw_types::PhotoId;
use grid::{GridState, Item, RowModel};
use settings::Settings;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

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

/// Where this machine keeps what is local to it and to one workspace (design note 001 §5.4).
/// This crate resolves no directory itself; `app` decides.
pub struct LocalPaths {
    /// The thumbnail cache (D-075).
    pub previews: PathBuf,
    /// What the import view remembers between launches.
    pub settings: PathBuf,
    /// Where an import job keeps the state that lets an interrupted one resume.
    pub import_state: PathBuf,
}

/// Asks a person for a folder: called with a dialog title, the folder to open the dialog at, and
/// what to do with the answer (`None` when the dialog was cancelled). Returns whether a dialog was
/// started. The answer is delivered on the interface thread. Tests without a display pass a
/// picker of their own: a native dialog cannot be opened without one.
pub type FolderPicker = Rc<dyn Fn(&str, Option<PathBuf>, Box<dyn FnOnce(Option<PathBuf>)>) -> bool>;

/// The system's own folder dialog (Windows and macOS: the native ones; Linux: the desktop portal),
/// run as a task of the event loop so the window stays alive and the dialog cannot freeze it.
fn native_folder_picker() -> FolderPicker {
    Rc::new(|title, start, done| {
        let title = title.to_string();
        slint::spawn_local(async move {
            let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
            if let Some(start) = start {
                dialog = dialog.set_directory(start);
            }
            done(
                dialog
                    .pick_folder()
                    .await
                    .map(|folder| folder.path().to_path_buf()),
            );
        })
        .is_ok()
    })
}

/// Where a folder dialog should open for what is typed in a field: the folder itself when it
/// exists, else the closest folder above it that does, else the system's default (`None`).
fn start_directory(typed: &str) -> Option<PathBuf> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    std::path::Path::new(typed)
        .ancestors()
        .find(|candidate| !candidate.as_os_str().is_empty() && candidate.is_dir())
        .map(std::path::Path::to_path_buf)
}

/// The import in progress, if any: which job, so its events are told apart from anything else.
#[derive(Default)]
struct ImportRun {
    job: Option<JobId>,
}

fn volume_entries(engine: &Engine) -> Vec<VolumeEntry> {
    engine
        .removable_volumes()
        .into_iter()
        .map(|v| VolumeEntry {
            label: format!("{} ({})", v.name, v.mount_point.display()).into(),
            path: v.mount_point.to_string_lossy().into_owned().into(),
        })
        .collect()
}

fn refresh_volumes(engine: &Engine, ui: &MainWindow) {
    let entries = volume_entries(engine);
    // A single camera card, and nothing typed yet: offer it (one click to import, spec §5.2).
    if ui.get_import_source().is_empty() && entries.len() == 1 {
        ui.set_import_source(entries[0].path.clone());
    }
    ui.set_volumes(ModelRc::new(VecModel::from(entries)));
}

fn field(text: SharedString) -> String {
    text.trim().to_string()
}

/// The fields of the import view, as settings (what is remembered, and what a profile is built
/// from).
fn settings_from(ui: &MainWindow) -> Settings {
    Settings {
        archive: field(ui.get_import_archive()),
        backup: field(ui.get_import_backup()),
        template: field(ui.get_import_template()),
        creator: field(ui.get_import_creator()),
        rights: field(ui.get_import_rights()),
        source: field(ui.get_import_source()),
    }
}

/// Reloads the grid's list from the catalogue and says how many photos there are.
fn reload(engine: &Engine, state: &GridState, ui: &MainWindow) {
    let items = load_items(engine, 0).unwrap_or_default();
    ui.set_status(ui.global::<Texts>().invoke_photos(items.len() as i32));
    ui.set_selected_summary(SharedString::new());
    state.set_items(items);
}

/// The shell's window and the timers that feed it; dropping the timers would stop the window from
/// ever hearing about a thumbnail or an import's progress.
struct Shell {
    ui: MainWindow,
    _timers: [Timer; 2],
}

/// Runs the shell until its window is closed.
pub fn run(
    engine: Engine,
    events: EventReceiver,
    paths: &LocalPaths,
) -> Result<(), slint::PlatformError> {
    build(engine, events, paths, native_folder_picker())?
        .ui
        .run()
}

/// Builds the window and connects it to the engine, without showing it: what `run` does, and what
/// the tests without a display drive directly.
fn build(
    engine: Engine,
    events: EventReceiver,
    paths: &LocalPaths,
    pick_folder: FolderPicker,
) -> Result<Shell, slint::PlatformError> {
    let ui = MainWindow::new()?;

    let state = Rc::new(GridState::default());
    let items = load_items(&engine, 0).unwrap_or_default();
    ui.set_status(ui.global::<Texts>().invoke_photos(items.len() as i32));
    state.set_items(items);

    let thumbnails = Rc::new(
        engine
            .start_thumbnails(&paths.previews, 4)
            .expect("the previews database can be opened"),
    );
    ui.set_rows(ModelRc::new(RowModel::new(
        state.clone(),
        thumbnails.clone(),
    )));

    let saved = Settings::load(&paths.settings);
    ui.set_import_archive(saved.archive.into());
    ui.set_import_backup(saved.backup.into());
    ui.set_import_template(saved.template.into());
    ui.set_import_creator(saved.creator.into());
    ui.set_import_rights(saved.rights.into());
    ui.set_import_source(saved.source.into());
    refresh_volumes(&engine, &ui);
    // A library with nothing in it yet opens on what fills it.
    if state.len() == 0 {
        ui.set_current_task("import".into());
    }

    // Thumbnails and engine events both arrive off the UI thread; a short repeating timer drains
    // each, the same shape spike 3 used for pictures arriving from its own worker threads.
    let thumbnail_timer = Timer::default();
    {
        let (state, thumbnails) = (state.clone(), thumbnails.clone());
        thumbnail_timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            for (id, thumbnail) in thumbnails.poll() {
                state.deliver(id, &thumbnail);
            }
            for id in thumbnails.poll_failed() {
                state.mark_unavailable(id);
            }
        });
    }

    let run = Rc::new(RefCell::new(ImportRun::default()));
    let new_photos = Rc::new(StdCell::new(false));
    let event_timer = Timer::default();
    {
        let (state, engine, weak) = (state.clone(), engine.clone(), ui.as_weak());
        let (run, new_photos) = (run.clone(), new_photos.clone());
        let mut last_reload = Instant::now();
        event_timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
            let Some(ui) = weak.upgrade() else { return };
            let texts = ui.global::<Texts>();
            while let Some(event) = events.try_recv() {
                let ours = |job: &JobId| run.borrow().job.as_ref() == Some(job);
                match event {
                    Event::PhotoChanged(id) => {
                        if let Some(row) = photo_row(&engine, id) {
                            state.set_rating(id, row.effective_rating);
                            if state.selected_id() == Some(id) {
                                ui.set_selected_summary(summary_of(&engine, id).into());
                            }
                        }
                    }
                    Event::ImportItem {
                        job,
                        photo_id: Some(_),
                        ..
                    } if ours(&job) => new_photos.set(true),
                    Event::JobProgress { job, done, total } if ours(&job) && total > 0 => {
                        ui.set_import_progress(done as f32 / total as f32);
                        ui.set_import_status(texts.invoke_importing(done as i32, total as i32));
                    }
                    Event::ImportFinished {
                        job,
                        copied,
                        skipped,
                        failed,
                    } if ours(&job) => {
                        run.borrow_mut().job = None;
                        ui.set_importing(false);
                        ui.set_import_finished(true);
                        ui.set_import_status(if skipped == 0 && failed == 0 {
                            texts.invoke_all_copied(copied as i32)
                        } else {
                            texts.invoke_import_summary(
                                copied as i32,
                                skipped as i32,
                                failed as i32,
                            )
                        });
                        new_photos.set(true);
                    }
                    Event::ImportAborted { job, reason } if ours(&job) => {
                        run.borrow_mut().job = None;
                        ui.set_importing(false);
                        ui.set_import_status(texts.invoke_import_stopped(reason.into()));
                    }
                    Event::JobCancelled(job) if ours(&job) => {
                        run.borrow_mut().job = None;
                        ui.set_importing(false);
                        ui.set_import_finished(true);
                        ui.set_import_status(texts.invoke_import_cancelled());
                        new_photos.set(true);
                    }
                    _ => {}
                }
            }
            // Photos an import has registered since the grid was last loaded: refresh it, at
            // most once a second, and only while it is the view on screen.
            if new_photos.get()
                && ui.get_current_task() == "cull"
                && last_reload.elapsed() > Duration::from_secs(1)
            {
                new_photos.set(false);
                last_reload = Instant::now();
                reload(&engine, &state, &ui);
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
            ui.set_status(ui.global::<Texts>().invoke_photos(items.len() as i32));
            state.set_items(items);
            ui.set_selected_summary(String::new().into());
        });
    }

    {
        let (engine, weak) = (engine.clone(), ui.as_weak());
        ui.on_refresh_volumes(move || {
            if let Some(ui) = weak.upgrade() {
                refresh_volumes(&engine, &ui);
            }
        });
    }

    {
        let (engine, state, weak) = (engine.clone(), state.clone(), ui.as_weak());
        ui.on_show_photos(move || {
            let Some(ui) = weak.upgrade() else { return };
            reload(&engine, &state, &ui);
            ui.set_current_task("cull".into());
        });
    }

    {
        let (picking, weak) = (Rc::new(StdCell::new(false)), ui.as_weak());
        ui.on_browse_folder(move |which| {
            let Some(ui) = weak.upgrade() else { return };
            if picking.get() {
                return;
            }
            let current = match which.as_str() {
                "source" => ui.get_import_source(),
                "archive" => ui.get_import_archive(),
                _ => ui.get_import_backup(),
            };
            let title = ui.global::<Texts>().invoke_pick_title(which.clone());
            picking.set(true);
            ui.set_picking(true);
            let answer = {
                let (picking, weak) = (picking.clone(), weak.clone());
                Box::new(move |chosen: Option<PathBuf>| {
                    picking.set(false);
                    let Some(ui) = weak.upgrade() else { return };
                    ui.set_picking(false);
                    let Some(folder) = chosen else { return };
                    let text: SharedString = folder.to_string_lossy().as_ref().into();
                    match which.as_str() {
                        "source" => ui.set_import_source(text),
                        "archive" => ui.set_import_archive(text),
                        _ => ui.set_import_backup(text),
                    }
                })
            };
            if !pick_folder(&title, start_directory(current.as_str()), answer) {
                picking.set(false);
                ui.set_picking(false);
            }
        });
    }

    {
        let (engine, run, weak) = (engine.clone(), run.clone(), ui.as_weak());
        let (settings_path, state_dir) = (paths.settings.clone(), paths.import_state.clone());
        ui.on_start_import(move || {
            let Some(ui) = weak.upgrade() else { return };
            let texts = ui.global::<Texts>();
            let fields = settings_from(&ui);
            fields.save(&settings_path);
            let request = ImportRequest {
                source_root: PathBuf::from(&fields.source),
                archive_root: PathBuf::from(&fields.archive),
                profile: fields.profile(),
                shoot: Some(field(ui.get_import_shoot())).filter(|s| !s.is_empty()),
                backup_root: Some(fields.backup.clone())
                    .filter(|b| !b.is_empty())
                    .map(PathBuf::from),
                state_dir: state_dir.clone(),
            };
            match engine.import(request) {
                Ok(job) => {
                    run.borrow_mut().job = Some(job);
                    ui.set_importing(true);
                    ui.set_import_finished(false);
                    ui.set_import_progress(0.0);
                    ui.set_import_status(texts.invoke_reading_card());
                }
                Err(e) => ui.set_import_status(texts.invoke_import_refused(e.to_string().into())),
            }
        });
    }

    {
        let (engine, run) = (engine.clone(), run.clone());
        ui.on_cancel_import(move || {
            if let Some(job_id) = run.borrow().job {
                let _ = engine.submit(Command::CancelJob { job_id });
            }
        });
    }

    Ok(Shell {
        ui,
        _timers: [thumbnail_timer, event_timer],
    })
}

/// The flat index arrow navigation lands on, clamped to the list's bounds.
fn next_index(state: &GridState, dx: i32, dy: i32, cols: i32) -> usize {
    let len = state.len() as i32;
    let current = state.current_selected_or_zero() as i32;
    (current + dx + dy * cols).clamp(0, (len - 1).max(0)) as usize
}
