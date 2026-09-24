// SPDX-License-Identifier: GPL-3.0-or-later
//! The window of an open workspace: the library grid, the import view (until the catalogue panel and
//! the import dialog replace it), and everything wired to that workspace's engine.

use std::cell::{Cell as StdCell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use auroraw_engine::{
    AddPlan, AddSourceRequest, Command, DestinationKind, Engine, Event, ImportRequest, JobId,
    OpenedWorkspace, SourceInfo, paths,
};
use auroraw_types::PhotoId;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::Platform;
use crate::app::{Launcher, Shell};
use crate::generated::{MainWindow, SourceEntry, Texts, VolumeEntry};
use crate::grid::{self, GridState, Item, RowModel};
use crate::settings::Settings;

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

/// What the catalogue panel is doing: the sources it lists, and the scan or removal in progress.
#[derive(Default)]
struct CatalogueState {
    sources: Vec<SourceInfo>,
    /// The scan or removal in progress: told apart from the import's own jobs.
    job: Option<JobId>,
    /// The source a removal is confirmed for, and its name for the closing message.
    removing: Option<(auroraw_types::SourceId, String)>,
    removing_name: String,
}

fn refresh_sources(engine: &Engine, ui: &MainWindow, state: &Rc<RefCell<CatalogueState>>) {
    let sources = engine.sources().unwrap_or_default();
    let entries: Vec<SourceEntry> = sources
        .iter()
        .map(|s| SourceEntry {
            name: s.name.as_str().into(),
            path: s.path.to_string_lossy().as_ref().into(),
            online: s.online,
            photos: s.photos as i32,
            worked_on: s.worked_on as i32,
        })
        .collect();
    ui.set_sources(ModelRc::new(VecModel::from(entries)));
    state.borrow_mut().sources = sources;
}

/// The import in progress, if any: which job, so its events are told apart from anything else.
#[derive(Default)]
struct ImportRun {
    job: Option<JobId>,
    /// A destination that was made a source for this import: scanned once the copy is done.
    added_source: Option<auroraw_types::SourceId>,
}

fn volume_entries(platform: &Platform) -> Vec<VolumeEntry> {
    (platform.volumes)()
        .into_iter()
        .map(|v| VolumeEntry {
            label: format!("{} ({})", v.name, v.mount_point.display()).into(),
            path: v.mount_point.to_string_lossy().into_owned().into(),
        })
        .collect()
}

fn refresh_volumes(platform: &Platform, ui: &MainWindow) {
    let entries = volume_entries(platform);
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
        destination: field(ui.get_import_destination()),
        backup: field(ui.get_import_backup()),
        template: field(ui.get_import_template()),
        creator: field(ui.get_import_creator()),
        rights: field(ui.get_import_rights()),
        source: field(ui.get_import_source()),
        layout: ui.get_import_layout().to_string(),
        add_destination: ui.get_import_add_destination(),
    }
}

/// Reloads the grid's list from the catalogue and says how many photos there are.
fn reload(engine: &Engine, state: &GridState, ui: &MainWindow) {
    let items = load_items(engine, 0).unwrap_or_default();
    ui.set_status(ui.global::<Texts>().invoke_photos(items.len() as i32));
    ui.set_selected_summary(SharedString::new());
    state.set_items(items);
}

/// Builds the window of an open workspace and connects it to its engine: what a person works in
/// once a workspace is open. `created` is a workspace that was just made, which opens on what fills
/// it rather than on an empty grid.
pub(crate) fn attach(
    launcher: &Rc<Launcher>,
    opened: OpenedWorkspace,
    created: bool,
) -> Result<Shell, slint::PlatformError> {
    let OpenedWorkspace {
        engine,
        events,
        workspace_id,
        name,
        previews_path,
        ..
    } = opened;
    let workspace_data = launcher.launch.dirs.workspace_data(workspace_id);
    let settings_path = workspace_data.join("import-settings.json");
    let state_dir = workspace_data.join("import");
    let ui = MainWindow::new()?;
    ui.set_screen("workspace".into());
    ui.set_workspace_name(name.into());

    let state = Rc::new(GridState::default());
    state.set_columns(ui.get_grid_columns() as usize);
    let items = load_items(&engine, 0).unwrap_or_default();
    ui.set_status(ui.global::<Texts>().invoke_photos(items.len() as i32));
    state.set_items(items);

    let thumbnails = Rc::new(
        engine
            .start_thumbnails(&previews_path, 4)
            .expect("the previews database can be opened"),
    );
    ui.set_rows(ModelRc::new(RowModel::new(
        state.clone(),
        thumbnails.clone(),
    )));

    let saved = Settings::load(&settings_path);
    ui.set_import_destination(saved.destination.into());
    ui.set_import_layout(saved.layout.as_str().into());
    ui.set_import_add_destination(saved.add_destination);
    ui.set_import_backup(saved.backup.into());
    ui.set_import_template(saved.template.into());
    ui.set_import_creator(saved.creator.into());
    ui.set_import_rights(saved.rights.into());
    ui.set_import_source(saved.source.into());
    refresh_volumes(&launcher.platform, &ui);
    // A workspace with nothing in it yet opens on what fills it: the catalogue's sources.
    let catalogue = Rc::new(RefCell::new(CatalogueState::default()));
    refresh_sources(&engine, &ui, &catalogue);
    if created || state.len() == 0 {
        ui.set_current_task("catalogue".into());
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
        let (catalogue, thumbnails) = (catalogue.clone(), thumbnails.clone());
        let mut last_reload = Instant::now();
        event_timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
            let Some(ui) = weak.upgrade() else { return };
            let texts = ui.global::<Texts>();
            while let Some(event) = events.try_recv() {
                let ours = |job: &JobId| run.borrow().job.as_ref() == Some(job);
                let scanning = |job: &JobId| catalogue.borrow().job.as_ref() == Some(job);
                match event {
                    Event::IndexPlanned {
                        job,
                        new_files,
                        restorable,
                        ..
                    } if scanning(&job) => {
                        if restorable > 0 {
                            ui.set_restore_count(restorable as i32);
                            ui.set_restore_total(new_files as i32);
                            ui.set_dialog("restore".into());
                        }
                    }
                    Event::JobProgress { job, done, total } if scanning(&job) && total > 0 => {
                        ui.set_catalogue_progress(done as f32 / total as f32);
                        ui.set_catalogue_status(texts.invoke_indexing(done as i32, total as i32));
                    }
                    Event::IndexFinished {
                        job,
                        added,
                        restored,
                        known,
                        failed,
                        ..
                    } if scanning(&job) => {
                        catalogue.borrow_mut().job = None;
                        ui.set_catalogue_busy(false);
                        ui.set_catalogue_status(texts.invoke_index_done(
                            added as i32,
                            restored as i32,
                            known as i32,
                            failed as i32,
                        ));
                        refresh_sources(&engine, &ui, &catalogue);
                        new_photos.set(true);
                    }
                    Event::IndexAborted { job, reason } if scanning(&job) => {
                        catalogue.borrow_mut().job = None;
                        ui.set_catalogue_busy(false);
                        ui.set_catalogue_status(texts.invoke_index_stopped(reason.into()));
                    }
                    Event::SourceRemoved { job, removed, .. } if scanning(&job) => {
                        let name = std::mem::take(&mut catalogue.borrow_mut().removing_name);
                        catalogue.borrow_mut().job = None;
                        ui.set_catalogue_busy(false);
                        ui.set_catalogue_status(
                            texts.invoke_source_removed(name.into(), removed as i32),
                        );
                        refresh_sources(&engine, &ui, &catalogue);
                        new_photos.set(true);
                    }
                    Event::JobCancelled(job) if scanning(&job) => {
                        catalogue.borrow_mut().job = None;
                        ui.set_catalogue_busy(false);
                        ui.set_catalogue_status(texts.invoke_scan_cancelled());
                        refresh_sources(&engine, &ui, &catalogue);
                        new_photos.set(true);
                    }
                    Event::PhotoChanged(id) => {
                        // A photo that just entered the catalogue gets its thumbnail made at once,
                        // in the order photos arrive, without waiting for a grid to show it.
                        thumbnails.warm(id);
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
                        // A destination made a source for this import: its other photos, if any,
                        // are scanned now (the imported ones are known already).
                        let added = run.borrow_mut().added_source.take();
                        if let Some(source_id) = added
                            && let Ok(auroraw_engine::Outcome::IndexStarted { job }) = engine
                                .submit_and_wait(Command::IndexSource {
                                    source_id,
                                    merge: Vec::new(),
                                })
                        {
                            catalogue.borrow_mut().job = Some(job);
                            ui.set_catalogue_busy(true);
                            ui.set_catalogue_progress(0.0);
                        }
                        refresh_sources(&engine, &ui, &catalogue);
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
            let next = grid::step(
                state.current_selected_or_zero(),
                dx,
                dy,
                state.columns(),
                state.len(),
            );
            select_and_reveal(&ui, &engine, &state, next);
        });
    }

    {
        let (state, engine, weak) = (state.clone(), engine.clone(), ui.as_weak());
        ui.on_jump_selection(move |kind| {
            let Some(ui) = weak.upgrade() else { return };
            let kind = match kind.as_str() {
                "page-up" => grid::Jump::PageUp,
                "page-down" => grid::Jump::PageDown,
                "home" => grid::Jump::Home,
                "end" => grid::Jump::End,
                _ => return,
            };
            if state.len() == 0 {
                return;
            }
            let next = grid::jump(
                kind,
                state.current_selected_or_zero(),
                state.columns(),
                ui.get_grid_visible_rows() as usize,
                state.len(),
            );
            select_and_reveal(&ui, &engine, &state, next);
        });
    }

    {
        // The window was resized: the rows are re-flowed to what fits, and the selection is
        // brought back into view since its row moved.
        let (state, weak) = (state.clone(), ui.as_weak());
        ui.on_columns_changed(move |cols| {
            state.set_columns(cols.max(1) as usize);
            if let (Some(ui), Some(selected)) = (weak.upgrade(), state.selected_index()) {
                reveal(&ui, &state, selected);
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
        let (platform, weak) = (launcher.platform.clone(), ui.as_weak());
        ui.on_refresh_volumes(move || {
            if let Some(ui) = weak.upgrade() {
                refresh_volumes(&platform, &ui);
            }
        });
    }

    {
        let (engine, state, weak) = (engine.clone(), state.clone(), ui.as_weak());
        ui.on_show_photos(move || {
            let Some(ui) = weak.upgrade() else { return };
            reload(&engine, &state, &ui);
            ui.set_dialog(SharedString::new());
            ui.set_current_task("cull".into());
        });
    }

    // The catalogue panel: adding, removing and rescanning sources.
    {
        let weak = ui.as_weak();
        ui.on_add_source(move || {
            let Some(ui) = weak.upgrade() else { return };
            ui.set_source_folder(SharedString::new());
            ui.set_source_name(SharedString::new());
            ui.set_add_source_error(SharedString::new());
            ui.set_add_source_merge_question(SharedString::new());
            ui.set_dialog("add-source".into());
        });
    }
    {
        let (engine, catalogue, weak) = (engine.clone(), catalogue.clone(), ui.as_weak());
        let add = move |merge: bool| {
            let Some(ui) = weak.upgrade() else { return };
            let texts = ui.global::<Texts>();
            let root = match paths::resolve(ui.get_source_folder().as_str()) {
                Ok(root) => root,
                Err(e) => {
                    ui.set_add_source_error(texts.invoke_cannot_add_source(e.to_string().into()));
                    return;
                }
            };
            // The dialog shows back what was understood.
            ui.set_source_folder(root.to_string_lossy().as_ref().into());
            ui.set_add_source_error(SharedString::new());
            if !merge && let Ok(AddPlan::ContainsExisting(inner)) = engine.plan_add_source(&root) {
                let names: Vec<String> = inner.iter().map(|s| format!("\"{}\"", s.name)).collect();
                ui.set_add_source_merge_question(
                    texts.invoke_merge_question(names.join(", ").into()),
                );
                return;
            }
            let name = ui.get_source_name().trim().to_string();
            match engine.add_source(AddSourceRequest {
                root,
                name: Some(name).filter(|n| !n.is_empty()),
                merge,
            }) {
                Ok(added) => {
                    catalogue.borrow_mut().job = Some(added.job);
                    ui.set_catalogue_busy(true);
                    ui.set_catalogue_progress(0.0);
                    ui.set_catalogue_status(texts.invoke_indexing(0, 0));
                    ui.set_dialog(SharedString::new());
                    refresh_sources(&engine, &ui, &catalogue);
                }
                Err(e) => {
                    ui.set_add_source_merge_question(SharedString::new());
                    ui.set_add_source_error(texts.invoke_cannot_add_source(e.to_string().into()));
                }
            }
        };
        let add = Rc::new(add);
        {
            let add = add.clone();
            ui.on_add_source_confirm(move || add(false));
        }
        ui.on_add_source_merge(move || add(true));
    }
    {
        let (catalogue, weak, engine) = (catalogue.clone(), ui.as_weak(), engine.clone());
        ui.on_remove_source(move |index| {
            let Some(ui) = weak.upgrade() else { return };
            let source = usize::try_from(index)
                .ok()
                .and_then(|i| catalogue.borrow().sources.get(i).cloned());
            let Some(source) = source else { return };
            let counts = engine.source_counts(source.id).ok();
            ui.set_remove_name(source.name.as_str().into());
            ui.set_remove_photos(counts.map_or(source.photos, |c| c.photos) as i32);
            ui.set_remove_worked_on(counts.map_or(source.worked_on, |c| c.worked_on) as i32);
            let mut state = catalogue.borrow_mut();
            state.removing_name = source.name.clone();
            state.removing = Some((source.id, source.name));
            ui.set_dialog("remove-source".into());
        });
    }
    {
        let (catalogue, weak, engine) = (catalogue.clone(), ui.as_weak(), engine.clone());
        ui.on_remove_source_confirm(move || {
            let Some(ui) = weak.upgrade() else { return };
            let Some((source_id, _)) = catalogue.borrow().removing.clone() else {
                return;
            };
            if let Ok(auroraw_engine::Outcome::RemoveStarted { job }) =
                engine.submit_and_wait(Command::RemoveSource { source_id })
            {
                catalogue.borrow_mut().job = Some(job);
                ui.set_catalogue_busy(true);
                ui.set_catalogue_progress(0.0);
                ui.set_catalogue_status(SharedString::new());
            }
            ui.set_dialog(SharedString::new());
        });
    }
    {
        let (catalogue, weak, engine) = (catalogue.clone(), ui.as_weak(), engine.clone());
        ui.on_rescan_source(move |index| {
            let Some(ui) = weak.upgrade() else { return };
            let source = usize::try_from(index)
                .ok()
                .and_then(|i| catalogue.borrow().sources.get(i).cloned());
            let Some(source) = source else { return };
            if let Ok(auroraw_engine::Outcome::IndexStarted { job }) =
                engine.submit_and_wait(Command::IndexSource {
                    source_id: source.id,
                    merge: Vec::new(),
                })
            {
                catalogue.borrow_mut().job = Some(job);
                ui.set_catalogue_busy(true);
                ui.set_catalogue_progress(0.0);
                ui.set_catalogue_status(ui.global::<Texts>().invoke_indexing(0, 0));
            }
        });
    }
    {
        let (catalogue, weak, engine) = (catalogue.clone(), ui.as_weak(), engine.clone());
        ui.on_restore_choice(move |restore| {
            let Some(ui) = weak.upgrade() else { return };
            if let Some(job_id) = catalogue.borrow().job {
                let _ = engine.submit(Command::ContinueIndex { job_id, restore });
            }
            ui.set_dialog(SharedString::new());
        });
    }
    {
        let (catalogue, weak, engine) = (catalogue.clone(), ui.as_weak(), engine.clone());
        ui.on_restore_cancelled(move || {
            let Some(ui) = weak.upgrade() else { return };
            if let Some(job_id) = catalogue.borrow().job {
                let _ = engine.submit(Command::CancelJob { job_id });
            }
            ui.set_dialog(SharedString::new());
        });
    }

    {
        let (engine, run, weak) = (engine.clone(), run.clone(), ui.as_weak());
        let (settings_path, state_dir) = (settings_path.clone(), state_dir.clone());
        let catalogue = catalogue.clone();
        ui.on_start_import(move || {
            let Some(ui) = weak.upgrade() else { return };
            let texts = ui.global::<Texts>();
            ui.invoke_import_fields_changed();
            let fields = settings_from(&ui);
            fields.save(&settings_path);
            let resolve = |typed: &str| paths::resolve(typed).map_err(|e| e.to_string());
            let request = (|| -> Result<ImportRequest, String> {
                Ok(ImportRequest {
                    source_root: resolve(&fields.source)?,
                    destination_root: resolve(&fields.destination)?,
                    profile: fields.profile(),
                    shoot: Some(field(ui.get_import_shoot())).filter(|s| !s.is_empty()),
                    backup_root: match fields.backup.as_str() {
                        "" => None,
                        backup => Some(resolve(backup)?),
                    },
                    state_dir: state_dir.clone(),
                    add_destination_as_source: fields.add_destination,
                })
            })();
            let request = match request {
                Ok(request) => request,
                Err(reason) => {
                    ui.set_import_status(texts.invoke_import_refused(reason.into()));
                    return;
                }
            };
            // The dialog shows back what was understood.
            ui.set_import_source(request.source_root.to_string_lossy().as_ref().into());
            ui.set_import_destination(request.destination_root.to_string_lossy().as_ref().into());
            match engine.import(request) {
                Ok(started) => {
                    let mut run = run.borrow_mut();
                    run.job = Some(started.job);
                    run.added_source = started.added_source;
                    ui.set_importing(true);
                    ui.set_import_finished(false);
                    ui.set_import_progress(0.0);
                    ui.set_import_status(texts.invoke_reading_card());
                    if started.added_source.is_some() {
                        refresh_sources(&engine, &ui, &catalogue);
                    }
                }
                Err(e) => ui.set_import_status(texts.invoke_import_refused(e.to_string().into())),
            }
        });
    }

    // What the import dialog's fields mean for the catalogue: whether the card keeps camera folders,
    // and what the destination is to the catalogue (which decides what is offered).
    {
        let (engine, weak) = (engine.clone(), ui.as_weak());
        ui.on_import_fields_changed(move || {
            let Some(ui) = weak.upgrade() else { return };
            let texts = ui.global::<Texts>();
            let info = paths::resolve(ui.get_import_source().as_str())
                .map(|source| Engine::inspect_import_source(&source))
                .unwrap_or_default();
            ui.set_import_source_has_folders(!info.camera_folders.is_empty());
            ui.set_import_source_folders(info.camera_folders.join(", ").into());
            let kind = paths::resolve(ui.get_import_destination().as_str())
                .ok()
                .and_then(|destination| engine.import_destination(&destination).ok());
            let (kind, note, registering) = match kind {
                None => ("", String::new(), false),
                Some(DestinationKind::Covered(source)) => (
                    "covered",
                    texts
                        .invoke_destination_covered(source.name.as_str().into())
                        .to_string(),
                    true,
                ),
                Some(DestinationKind::NotCovered) => {
                    let adding = ui.get_import_add_destination();
                    (
                        "not-covered",
                        texts.invoke_destination_not_covered(adding).to_string(),
                        adding,
                    )
                }
                Some(DestinationKind::ContainsSources(sources)) => {
                    let names: Vec<String> =
                        sources.iter().map(|s| format!("\"{}\"", s.name)).collect();
                    (
                        "contains",
                        texts
                            .invoke_destination_contains(names.join(", ").into())
                            .to_string(),
                        false,
                    )
                }
            };
            ui.set_import_destination_kind(kind.into());
            ui.set_import_destination_note(note.into());
            ui.set_import_registering(registering);
        });
    }
    // A card inserted while the application runs (one with a DCIM folder) is offered for import.
    let card_timer = Timer::default();
    {
        let platform = launcher.platform.clone();
        let known: Rc<RefCell<std::collections::HashSet<PathBuf>>> = Rc::new(RefCell::new(
            (platform.volumes)()
                .into_iter()
                .map(|v| v.mount_point)
                .collect(),
        ));
        let pending: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
        {
            let (pending, weak) = (pending.clone(), ui.as_weak());
            ui.on_card_import(move || {
                let Some(ui) = weak.upgrade() else { return };
                if let Some(card) = pending.borrow_mut().take() {
                    ui.set_import_source(card.to_string_lossy().as_ref().into());
                    ui.invoke_refresh_volumes();
                    ui.invoke_import_fields_changed();
                    ui.set_dialog("import".into());
                }
                ui.set_card_banner(SharedString::new());
            });
        }
        {
            let (pending, weak) = (pending.clone(), ui.as_weak());
            ui.on_card_dismiss(move || {
                pending.borrow_mut().take();
                if let Some(ui) = weak.upgrade() {
                    ui.set_card_banner(SharedString::new());
                }
            });
        }
        let weak = ui.as_weak();
        card_timer.start(TimerMode::Repeated, Duration::from_secs(2), move || {
            let Some(ui) = weak.upgrade() else { return };
            let volumes = (platform.volumes)();
            let mut known = known.borrow_mut();
            if let Some(card) = volumes
                .iter()
                .find(|v| v.has_dcim && !known.contains(&v.mount_point))
            {
                *pending.borrow_mut() = Some(card.mount_point.clone());
                ui.set_card_banner(
                    ui.global::<Texts>()
                        .invoke_card_detected(card.name.as_str().into()),
                );
            }
            *known = volumes.into_iter().map(|v| v.mount_point).collect();
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

    Ok(Shell::new(
        ui,
        vec![thumbnail_timer, event_timer, card_timer],
    ))
}

/// The flat index arrow navigation lands on, clamped to the list's bounds.
/// A row of the grid is this many logical pixels high (`ui/shell.slint`'s cell height and spacing).
const ROW_HEIGHT: f32 = 124.0;

/// Scrolls the grid the least that puts the row of `index` in view.
fn reveal(ui: &MainWindow, state: &GridState, index: usize) {
    let top = state.row_of(index) as f32 * ROW_HEIGHT;
    let bottom = top + ROW_HEIGHT;
    let view = ui.get_grid_view_height();
    let scrolled = -ui.get_grid_scroll();
    let target = if top < scrolled {
        top
    } else if bottom > scrolled + view {
        bottom - view
    } else {
        return;
    };
    ui.set_grid_scroll(-target.max(0.0));
}

/// Selects `index`, shows its summary and keeps it in view (a keyboard move).
fn select_and_reveal(ui: &MainWindow, engine: &Engine, state: &GridState, index: usize) {
    if let Some(id) = state.select(Some(index)) {
        ui.set_selected_summary(summary_of(engine, id).into());
    }
    reveal(ui, state, index);
}
