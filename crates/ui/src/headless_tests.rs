// SPDX-License-Identifier: GPL-3.0-or-later
//! The shell driven without a window or a display (testing strategy §6): Slint's testing backend
//! stands in for the windowing system, elements are found by their accessible labels the way a
//! screen reader would find them, clicks are real pointer events at an element's centre, and time
//! is advanced by hand. Nothing here touches anyone's desktop, and the same tests run on every CI
//! platform.
//!
//! What this cannot cover, and is checked on a real machine before a release instead: GPU
//! rendering, the platform's input methods, real fonts and scaling, frame rate.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use auroraw_catalogue::Catalogue;
use auroraw_engine::{Engine, LocalDirs, VolumeInfo};
use i_slint_backend_testing::{
    AccessibleRole, ElementHandle, ElementQuery, TestingBackend, TestingBackendOptions,
    init_no_event_loop, mock_elapsed_time,
};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model};

use crate::app::Launcher;
use crate::commands::COMMANDS;
use crate::generated::{MainWindow, Texts};
use crate::{FolderPicker, Launch, Platform, start_directory};

/// A machine: its data and cache folders, its Pictures folder, a folder of photos standing in for
/// a card, and where a workspace would go.
struct Fixture {
    _dir: auroraw_testkit::TempDir,
    dirs: LocalDirs,
    pictures: PathBuf,
    workspace: PathBuf,
    card: PathBuf,
    archive: PathBuf,
}

fn write_jpeg(path: &Path, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let img = ImageBuffer::from_fn(64, 48, |x, y| {
        Rgb([(x * 4) as u8 ^ seed, (y * 5) as u8, seed.wrapping_mul(7)])
    });
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, ImageFormat::Jpeg)
        .unwrap();
}

fn fixture(photos: u8) -> Fixture {
    let dir = auroraw_testkit::temp_dir();
    let card = dir.path().join("Card");
    std::fs::create_dir_all(&card).unwrap();
    for n in 0..photos {
        write_jpeg(&card.join(format!("IMG_{n:04}.jpg")), n + 1);
    }
    Fixture {
        dirs: LocalDirs {
            data: dir.path().join("data"),
            cache: dir.path().join("cache"),
        },
        pictures: dir.path().join("Pictures"),
        workspace: dir.path().join("Workspaces/Test"),
        archive: dir.path().join("Archive"),
        card,
        _dir: dir,
    }
}

/// The application, as a person meets it: whatever window is on screen.
struct App {
    launcher: Rc<Launcher>,
}

impl App {
    fn ui(&self) -> MainWindow {
        self.launcher.window()
    }
}

/// A folder picker that must never be reached: a test that does not browse.
fn no_dialog() -> FolderPicker {
    Rc::new(|_, _, _| panic!("no folder dialog was expected"))
}

fn platform(pick_folder: FolderPicker) -> Platform {
    Platform {
        pick_folder,
        volumes: Rc::new(Vec::new),
    }
}

/// Launches the application on `fixture`'s machine, in `language` (chosen after the first window
/// exists: the bundled translations only do once a window has been built). Opens the last workspace,
/// or the welcome list on a first launch.
fn launch_in(fixture: &Fixture, language: &str, platform: Platform) -> App {
    let launcher = Launcher::start(
        Launch {
            dirs: fixture.dirs.clone(),
            pictures: fixture.pictures.clone(),
            open: None,
        },
        platform,
    )
    .unwrap();
    slint::select_bundled_translation(language).unwrap();
    // Elements under a condition only exist once the window has been laid out.
    launcher.window().show().unwrap();
    App { launcher }
}

fn launch(fixture: &Fixture) -> App {
    launch_in(fixture, "en", platform(no_dialog()))
}

/// A workspace at `fixture.workspace`, made the way a person makes one: from the welcome list
/// of a first launch, through the New workspace dialog. Leaves the application on it.
fn open(fixture: &Fixture) -> App {
    open_in(fixture, "en")
}

fn open_in(fixture: &Fixture, language: &str) -> App {
    open_with(fixture, language, platform(no_dialog()))
}

fn open_with(fixture: &Fixture, language: &str, platform: Platform) -> App {
    let app = launch_in(fixture, language, platform);
    assert_eq!(app.ui().get_screen(), "welcome", "a first launch");
    let new_workspace = if language == "fr" {
        "Nouveau workspace…"
    } else {
        "New workspace…"
    };
    click(&app, new_workspace);
    let folder = if language == "fr" {
        "Dossier"
    } else {
        "Folder"
    };
    type_into(&app, folder, &fixture.workspace.to_string_lossy());
    click(&app, if language == "fr" { "Créer" } else { "Create" });
    assert_eq!(app.ui().get_screen(), "workspace");
    app.ui().show().unwrap();
    app
}

/// Where an import job of the workspace opened last keeps the state that lets it resume.
fn import_state(fixture: &Fixture) -> PathBuf {
    let id = Engine::last_opened_workspace(&fixture.dirs)
        .expect("a workspace was opened")
        .workspace_id;
    fixture.dirs.workspace_data(id).join("import")
}

/// Goes to the import view (File > Import; until the import dialog replaces it): where the import
/// tests start.
fn to_import(app: App) -> App {
    app.launcher.run_command(&app.ui(), "file.import");
    assert_eq!(app.ui().get_dialog(), "import");
    app
}

/// The catalogue of the workspace the fixture's machine opened last, for reading.
fn catalogue(fixture: &Fixture) -> Catalogue {
    let id = Engine::last_opened_workspace(&fixture.dirs)
        .expect("a workspace was opened")
        .workspace_id;
    Catalogue::open(&fixture.dirs.catalogue_path(id)).unwrap()
}

/// The testing backend for this thread.
fn init() {
    init_no_event_loop();
}

/// Types `text` into the field labelled `label`, the way an assistive technology sets a value.
fn type_into(shell: &App, label: &str, text: &str) {
    let field = ElementHandle::find_by_accessible_label(&shell.ui(), label)
        .find(|element| element.accessible_value().is_some())
        .unwrap_or_else(|| panic!("no text field labelled {label:?}"));
    field.set_accessible_value(text);
}

/// Clicks the button labelled `label`: a real pointer press and release at its centre.
fn click(shell: &App, label: &str) {
    let button = ElementQuery::from_root(&shell.ui())
        .match_descendants()
        .match_accessible_role(AccessibleRole::Button)
        .match_predicate({
            let label = label.to_string();
            move |element| element.accessible_label().is_some_and(|l| l == label)
        })
        .find_first()
        .unwrap_or_else(|| panic!("no button labelled {label:?}"));
    button.mock_single_click(PointerEventButton::Left);
}

/// Advances the simulated clock (which fires the shell's timers) until `done`, giving background
/// threads real time to work in between.
fn settle(what: &str, done: impl Fn() -> bool) {
    for _ in 0..1500 {
        mock_elapsed_time(Duration::from_millis(60));
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

fn fill_import_form(shell: &App, f: &Fixture) {
    type_into(
        shell,
        "Import from (card or folder)",
        &f.card.to_string_lossy(),
    );
    type_into(shell, "Destination folder", &f.archive.to_string_lossy());
}

#[test]
fn a_workspace_that_is_empty_opens_on_what_fills_it_and_one_with_photos_on_the_grid() {
    init();
    let f = fixture(2);
    {
        let shell = open(&f);
        assert_eq!(shell.ui().get_current_task(), "catalogue");
        let shell = to_import(shell);
        fill_import_form(&shell, &f);
        click(&shell, "Import");
        settle("the import", || shell.ui().get_import_finished());
    }
    // The next launch reopens it, on the grid, because it now has photos.
    let shell = launch(&f);
    assert_eq!(shell.ui().get_screen(), "workspace");
    assert_eq!(shell.ui().get_current_task(), "cull");
    assert_eq!(shell.ui().get_status(), "2 photos");
}

#[test]
fn filling_the_form_and_clicking_import_copies_verifies_and_shows_the_photos() {
    init();
    let f = fixture(3);
    let shell = to_import(open(&f));
    fill_import_form(&shell, &f);
    type_into(&shell, "Creator", "Patrick Fournier");
    assert_eq!(
        shell.ui().get_import_destination(),
        f.archive.to_string_lossy()
    );

    click(&shell, "Import");
    assert!(shell.ui().get_importing(), "the import started");
    settle("the import to finish", || shell.ui().get_import_finished());

    assert!(!shell.ui().get_importing());
    assert_eq!(
        shell.ui().get_import_status(),
        "All 3 files copied and verified."
    );
    for n in 0..3 {
        assert!(
            f.archive.join(format!("IMG_000{n}.jpg")).is_file(),
            "IMG_000{n}.jpg reached the archive (no date folders: the photos carry no capture time)"
        );
    }
    assert!(
        std::fs::read_dir(import_state(&f))
            .unwrap()
            .next()
            .is_none(),
        "a clean import leaves no state to resume"
    );

    // "Show photos" switches to the grid with what was just imported.
    click(&shell, "Show photos");
    assert_eq!(shell.ui().get_current_task(), "cull");
    assert_eq!(shell.ui().get_status(), "3 photos");
    assert_eq!(shell.ui().get_rows().row_count(), 1);

    // Thumbnails arrive from the background workers and fill the cells.
    settle("the thumbnails", || {
        let row = shell.ui().get_rows().row_data(0).unwrap();
        (0..3).all(|i| row.cells.row_data(i).unwrap().ready)
    });
}

#[test]
fn a_second_import_of_the_same_card_says_so_and_copies_nothing() {
    init();
    let f = fixture(2);
    let shell = to_import(open(&f));
    fill_import_form(&shell, &f);
    click(&shell, "Import");
    settle("the first import", || shell.ui().get_import_finished());

    click(&shell, "Import");
    // The flag is reset by the click and set again when the second job ends.
    settle("the second import", || {
        !shell.ui().get_importing() && shell.ui().get_import_finished()
    });
    assert_eq!(
        shell.ui().get_import_status(),
        "0 copied, 2 already in the library, 0 failed. Run it again to retry."
    );
}

#[test]
fn an_import_that_cannot_start_says_why_and_starts_nothing() {
    init();
    let f = fixture(1);
    let shell = to_import(open(&f));

    click(&shell, "Import");
    assert!(!shell.ui().get_importing());
    assert!(
        shell
            .ui()
            .get_import_status()
            .starts_with("Cannot start the import:"),
        "{}",
        shell.ui().get_import_status()
    );

    type_into(
        &shell,
        "Import from (card or folder)",
        &f.card.join("nowhere-at-all").to_string_lossy(),
    );
    type_into(&shell, "Destination folder", &f.archive.to_string_lossy());
    click(&shell, "Import");
    assert!(!shell.ui().get_importing());
    assert!(shell.ui().get_import_status().contains("is not a folder"));
    assert!(
        !f.archive.exists(),
        "nothing was created for a refused import"
    );
}

#[test]
fn the_form_is_remembered_the_next_time_the_workspace_opens() {
    init();
    let f = fixture(1);
    {
        let shell = to_import(open(&f));
        fill_import_form(&shell, &f);
        type_into(&shell, "Creator", "Patrick Fournier");
        type_into(&shell, "Copyright", "© Patrick Fournier");
        click(&shell, "Import");
        settle("the import", || shell.ui().get_import_finished());
    }
    let shell = launch(&f);
    assert_eq!(
        shell.ui().get_import_destination(),
        f.archive.to_string_lossy()
    );
    assert_eq!(shell.ui().get_import_source(), f.card.to_string_lossy());
    assert_eq!(shell.ui().get_import_creator(), "Patrick Fournier");
    assert_eq!(shell.ui().get_import_rights(), "© Patrick Fournier");
}

#[test]
fn rating_from_the_keyboard_reaches_the_catalogue() {
    init();
    let f = fixture(2);
    let shell = to_import(open(&f));
    fill_import_form(&shell, &f);
    click(&shell, "Import");
    settle("the import", || shell.ui().get_import_finished());
    click(&shell, "Show photos");

    click_cell(&shell, 1);
    press(&shell, "4");
    assert_eq!(
        shell
            .ui()
            .get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(1)
            .unwrap()
            .rating,
        4,
        "the clicked cell shows its new rating at once"
    );
    settle("the rating to be stored", || {
        catalogue(&f).list_by_min_rating(4, None, 10).unwrap().len() == 1
    });
}

/// A key press and release, as the window's own event loop would deliver them.
fn press(shell: &App, text: &str) {
    let ui = shell.ui();
    ui.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
    ui.window()
        .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
}

/// Clicks the n-th cell of the grid's first row.
fn click_cell(shell: &App, n: usize) {
    // A cell is 160 x 120 logical pixels with 4 between them, below the two 36 px bars.
    let x = 4.0 + n as f32 * 164.0 + 80.0;
    let y = 72.0 + 60.0;
    let position = slint::LogicalPosition::new(x, y);
    shell
        .ui()
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    shell
        .ui()
        .window()
        .dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
    shell
        .ui()
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
}

#[test]
fn run_time_sentences_are_translated_with_their_plural_forms() {
    init();
    let f = fixture(0);
    let shell = open(&f);
    let ui = shell.ui();
    let texts = ui.global::<Texts>();
    assert_eq!(texts.invoke_photos(1), "1 photo");
    assert_eq!(texts.invoke_photos(2), "2 photos");
    assert_eq!(
        texts.invoke_all_copied(1),
        "All 1 file copied and verified."
    );

    slint::select_bundled_translation("fr").unwrap();
    assert_eq!(texts.invoke_photos(0), "0 photo");
    assert_eq!(texts.invoke_photos(1), "1 photo");
    assert_eq!(texts.invoke_photos(2), "2 photos");
    assert_eq!(
        texts.invoke_all_copied(1),
        "Le fichier a été copié et vérifié."
    );
    assert_eq!(
        texts.invoke_all_copied(5),
        "Les 5 fichiers ont été copiés et vérifiés."
    );
    assert_eq!(texts.invoke_importing(3, 8), "Import de 3 sur 8…");
}

/// The testing backend with a real software rasteriser, so a window can be drawn without a
/// display.
fn init_rendering() {
    let backend = TestingBackend::new(TestingBackendOptions {
        mock_time: true,
        threading: false,
        renderer_name: Some("software".into()),
    });
    slint::platform::set_platform(Box::new(backend)).expect("one platform per test thread");
}

/// Draws the window at its real size and, when `AUR_SNAPSHOT_DIR` names a folder, writes it there
/// as `name.png` (how layout is looked at without touching a display). Always checks that
/// something other than one flat colour was drawn.
fn snapshot(shell: &App, name: &str) {
    shell
        .ui()
        .window()
        .set_size(slint::PhysicalSize::new(1400, 900));
    shell.ui().show().unwrap();
    let picture = shell.ui().window().take_snapshot().expect("a picture");
    let pixels = picture.as_bytes();
    assert!(
        pixels.chunks(4).any(|p| p != &pixels[..4]),
        "{name}: only one colour was drawn"
    );
    if let Some(dir) = std::env::var_os("AUR_SNAPSHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        image::save_buffer(
            Path::new(&dir).join(format!("{name}.png")),
            pixels,
            picture.width(),
            picture.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn the_views_render_and_can_be_written_out_as_pictures() {
    init_rendering();
    let f = fixture(10);
    let welcome = launch_in(&f, "fr", platform(no_dialog()));
    snapshot(&welcome, "welcome-empty");
    click(&welcome, "Nouveau workspace…");
    snapshot(&welcome, "new-workspace-dialog");
    click(&welcome, "Annuler");
    menu_open(&welcome);
    click_on_top(&welcome, "Édition");
    snapshot(&welcome, "menu-edit");
    welcome.ui().set_menu_open(false);
    welcome.launcher.run_command(&welcome.ui(), "help.about");
    snapshot(&welcome, "about");
    drop(welcome);
    let shell = to_import(open_in(&f, "fr"));
    snapshot(&shell, "import-empty");

    // The labels are French here, so the fields are set directly rather than found by label.
    shell
        .ui()
        .set_import_source(f.card.to_string_lossy().as_ref().into());
    shell
        .ui()
        .set_import_destination(f.archive.to_string_lossy().as_ref().into());
    click(&shell, "Import");
    settle("the import", || shell.ui().get_import_finished());
    snapshot(&shell, "import-done");

    click(&shell, "Voir les photos");
    settle("the thumbnails", || {
        let row = shell.ui().get_rows().row_data(0).unwrap();
        (0..8).all(|i| row.cells.row_data(i).unwrap().ready)
    });
    snapshot(&shell, "grid");
}

/// What a stand-in folder dialog was asked, and the answers it will give.
#[derive(Default)]
struct Dialogs {
    asked: Vec<(String, Option<PathBuf>)>,
    answer: Option<PathBuf>,
    /// Holds the answer back until the test delivers it, like a dialog that is still open.
    held: Option<Box<dyn FnOnce(Option<PathBuf>)>>,
    hold: bool,
}

fn stand_in(dialogs: &Rc<RefCell<Dialogs>>) -> FolderPicker {
    let dialogs = dialogs.clone();
    Rc::new(move |title, start, done| {
        let mut state = dialogs.borrow_mut();
        state.asked.push((title.to_string(), start));
        if state.hold {
            state.held = Some(done);
        } else {
            let answer = state.answer.clone();
            drop(state);
            done(answer);
        }
        true
    })
}

#[test]
fn browsing_fills_the_field_and_opens_the_dialog_where_the_field_points() {
    init();
    let f = fixture(0);
    let dialogs = Rc::new(RefCell::new(Dialogs::default()));
    let shell = to_import(open_with(&f, "en", platform(stand_in(&dialogs))));

    // An empty field opens the system's default place; the answer fills the field.
    dialogs.borrow_mut().answer = Some(f.archive.clone());
    click(&shell, "Browse for: Destination folder");
    assert_eq!(
        shell.ui().get_import_destination(),
        f.archive.to_string_lossy()
    );
    assert_eq!(
        dialogs.borrow().asked[0],
        ("Choose the archive folder".to_string(), None)
    );

    // A field that already names a folder opens the dialog there; each field has its own title.
    dialogs.borrow_mut().answer = Some(f.card.clone());
    type_into(
        &shell,
        "Import from (card or folder)",
        &f.workspace.to_string_lossy(),
    );
    std::fs::create_dir_all(&f.workspace).unwrap();
    click(&shell, "Browse for: Import from (card or folder)");
    assert_eq!(shell.ui().get_import_source(), f.card.to_string_lossy());
    assert_eq!(
        dialogs.borrow().asked[1],
        (
            "Choose the card or folder to import from".to_string(),
            Some(f.workspace.clone())
        )
    );

    dialogs.borrow_mut().answer = Some(f.workspace.clone());
    click(&shell, "Browse for: Backup folder (optional)");
    assert_eq!(
        shell.ui().get_import_backup(),
        f.workspace.to_string_lossy()
    );
    assert_eq!(dialogs.borrow().asked[2].0, "Choose the backup folder");
}

#[test]
fn cancelling_the_dialog_leaves_the_field_alone() {
    init();
    let f = fixture(0);
    let dialogs = Rc::new(RefCell::new(Dialogs::default()));
    let shell = to_import(open_with(&f, "en", platform(stand_in(&dialogs))));
    type_into(&shell, "Destination folder", "/kept/as/typed");

    dialogs.borrow_mut().answer = None;
    click(&shell, "Browse for: Destination folder");
    assert_eq!(shell.ui().get_import_destination(), "/kept/as/typed");
    assert!(!shell.ui().get_picking(), "the buttons are usable again");
}

#[test]
fn while_a_dialog_is_open_another_one_cannot_be_started() {
    init();
    let f = fixture(0);
    let dialogs = Rc::new(RefCell::new(Dialogs {
        hold: true,
        ..Dialogs::default()
    }));
    let shell = to_import(open_with(&f, "en", platform(stand_in(&dialogs))));

    click(&shell, "Browse for: Destination folder");
    assert!(shell.ui().get_picking());
    click(&shell, "Browse for: Import from (card or folder)");
    assert_eq!(
        dialogs.borrow().asked.len(),
        1,
        "the second click did nothing"
    );

    let done = dialogs.borrow_mut().held.take().unwrap();
    done(Some(f.archive.clone()));
    assert!(!shell.ui().get_picking());
    assert_eq!(
        shell.ui().get_import_destination(),
        f.archive.to_string_lossy()
    );
}

#[test]
fn the_dialog_opens_at_the_closest_folder_that_exists() {
    let dir = auroraw_testkit::temp_dir();
    let existing = dir.path().join("Photos");
    std::fs::create_dir_all(&existing).unwrap();

    assert_eq!(start_directory(""), None);
    assert_eq!(start_directory("   "), None);
    assert_eq!(
        start_directory(&existing.to_string_lossy()),
        Some(existing.clone())
    );
    assert_eq!(
        start_directory(&existing.join("2026/September/half-typed").to_string_lossy()),
        Some(existing),
        "the folders that do not exist yet are skipped"
    );
}

/// The welcome list's rows are buttons named "<name>, <folder>".
fn click_known(app: &App, name: &str, folder: &Path) {
    click(app, &format!("{name}, {}", canonical(folder).display()));
}

/// Renames a folder once the engine that had it open has let go of it (Windows refuses to move a
/// folder holding an open file, and a closed workspace is let go of a moment after it is dropped).
fn rename_when_free(from: &Path, to: &Path) {
    for _ in 0..50 {
        if std::fs::rename(from, to).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    std::fs::rename(from, to).unwrap();
}

/// A folder as the application stores and shows it: canonical (on a Mac the temporary folder of a
/// test is behind a symlink, on Windows it has a verbatim prefix).
fn canonical(folder: &Path) -> PathBuf {
    auroraw_engine::paths::resolve(&folder.to_string_lossy()).unwrap()
}

#[test]
fn a_first_launch_offers_to_create_or_open_a_workspace() {
    init();
    let f = fixture(0);
    let app = launch(&f);
    let ui = app.ui();
    assert_eq!(ui.get_screen(), "welcome");
    assert_eq!(ui.get_known().row_count(), 0, "nothing is known yet");
    // Both ways are there, and the second opens nothing until a folder is chosen.
    assert!(ElementHandle::find_by_accessible_label(&ui, "Open workspace…").count() > 0);
    click(&app, "New workspace…");
    assert_eq!(app.ui().get_dialog(), "new-workspace");
}

#[test]
fn creating_a_workspace_from_the_dialog_opens_it_and_remembers_it() {
    init();
    let f = fixture(0);
    let app = launch(&f);
    click(&app, "New workspace…");
    // The dialog proposes <Pictures>/Auroraw/<name>, and the name is translated ("Main").
    assert_eq!(app.ui().get_dialog(), "new-workspace");
    assert_eq!(app.ui().get_new_name(), "Main");
    assert_eq!(
        app.ui().get_new_location(),
        f.pictures.join("Auroraw").join("Main").to_string_lossy()
    );

    click(&app, "Create");
    let ui = app.ui();
    assert_eq!(ui.get_screen(), "workspace");
    assert_eq!(ui.get_workspace_name(), "Main");
    assert_eq!(
        ui.get_current_task(),
        "catalogue",
        "a new workspace opens on what fills it"
    );
    assert!(
        f.pictures
            .join("Auroraw")
            .join("Main")
            .join("workspace.json")
            .is_file()
    );
    let known = Engine::known_workspaces(&f.dirs);
    assert_eq!(known.len(), 1);
    assert_eq!(known[0].name, "Main");
}

#[test]
fn the_proposed_folder_follows_the_name_until_the_folder_is_edited() {
    init();
    let f = fixture(0);
    let app = launch(&f);
    click(&app, "New workspace…");

    type_into(&app, "Name", "Family");
    assert_eq!(
        app.ui().get_new_location(),
        f.pictures.join("Auroraw").join("Family").to_string_lossy()
    );
    type_into(&app, "Name", "Family / trips: 2026");
    assert_eq!(
        app.ui().get_new_location(),
        f.pictures
            .join("Auroraw/Family _ trips_ 2026")
            .to_string_lossy(),
        "what a file system refuses is replaced"
    );

    // A folder chosen by hand is not overwritten by a later change of name.
    type_into(&app, "Folder", &f.workspace.to_string_lossy());
    type_into(&app, "Name", "Renamed");
    assert_eq!(app.ui().get_new_location(), f.workspace.to_string_lossy());
}

#[test]
fn a_taken_name_is_offered_with_a_number() {
    init();
    let f = fixture(0);
    std::fs::create_dir_all(f.pictures.join("Auroraw").join("Main")).unwrap();
    let app = launch(&f);
    click(&app, "New workspace…");
    assert_eq!(
        app.ui().get_new_location(),
        f.pictures.join("Auroraw").join("Main 2").to_string_lossy()
    );
}

#[test]
fn a_workspace_that_cannot_be_created_says_why_and_the_dialog_stays() {
    init();
    let f = fixture(0);
    let app = launch(&f);
    click(&app, "New workspace…");

    type_into(&app, "Name", "   ");
    click(&app, "Create");
    assert_eq!(app.ui().get_dialog_error(), "Give the workspace a name.");

    type_into(&app, "Name", "Main");
    type_into(&app, "Folder", "photos/relative");
    click(&app, "Create");
    assert!(
        app.ui()
            .get_dialog_error()
            .starts_with("Cannot create the workspace:"),
        "{}",
        app.ui().get_dialog_error()
    );
    assert!(app.ui().get_dialog_error().contains("absolute"));
    assert_eq!(app.ui().get_screen(), "welcome");
    assert_eq!(app.ui().get_dialog(), "new-workspace");
}

#[test]
fn a_typed_folder_is_shown_back_canonical() {
    init();
    let f = fixture(0);
    let app = launch(&f);
    click(&app, "New workspace…");
    // A path with `..` and a repeated separator is what a person may type; the dialog shows what
    // was understood (and the workspace is created there, not in a folder named "..").
    let typed = format!(
        "{}//Workspaces/x/../Test/",
        f.workspace.parent().unwrap().parent().unwrap().display()
    );
    type_into(&app, "Folder", &typed);
    click(&app, "Create");
    assert_eq!(app.ui().get_screen(), "workspace");
    assert!(f.workspace.join("workspace.json").is_file());
}

#[test]
fn the_last_workspace_reopens_on_the_next_launch() {
    init();
    let f = fixture(0);
    drop(open(&f));
    let app = launch(&f);
    assert_eq!(
        app.ui().get_screen(),
        "workspace",
        "{}",
        app.ui().get_notice()
    );
    assert_eq!(app.ui().get_workspace_name(), "Main");
}

#[test]
fn a_last_workspace_that_cannot_be_found_leads_back_to_the_welcome_list() {
    init();
    let f = fixture(0);
    drop(open(&f));
    rename_when_free(&f.workspace, &f.workspace.with_file_name("Moved"));

    let app = launch(&f);
    let ui = app.ui();
    assert_eq!(ui.get_screen(), "welcome");
    assert!(
        ui.get_welcome_note()
            .starts_with("The last workspace could not be found:"),
        "{}",
        ui.get_welcome_note()
    );
    let known = ui.get_known();
    assert_eq!(known.row_count(), 1);
    assert!(!known.row_data(0).unwrap().found);

    // It can be taken off the list.
    click(&app, "Remove from the list: Main");
    settle("the list to be shown again", || {
        app.ui().get_known().row_count() == 0
    });
}

/// Two workspaces on one machine, the second (opened last) then moved out of sight: the next
/// launch cannot reopen it and shows the welcome list. Returns where it went.
fn machine_with_a_lost_last_workspace(f: &Fixture) -> PathBuf {
    drop(open(f));
    let second = f.workspace.with_file_name("Second");
    drop(
        Engine::create_workspace(&second, "Second", &f.dirs)
            .unwrap()
            .engine,
    );
    let hidden = f.workspace.with_file_name("Hidden");
    rename_when_free(&second, &hidden);
    hidden
}

#[test]
fn a_known_workspace_opens_from_the_list() {
    init();
    let f = fixture(0);
    machine_with_a_lost_last_workspace(&f);

    let app = launch(&f);
    assert_eq!(app.ui().get_screen(), "welcome");
    assert_eq!(app.ui().get_known().row_count(), 2);
    click_known(&app, "Main", &f.workspace);
    assert_eq!(app.ui().get_screen(), "workspace");
    assert_eq!(app.ui().get_workspace_name(), "Main");
}

#[test]
fn a_workspace_opens_from_a_folder_chosen_in_the_dialog_and_a_plain_folder_is_refused() {
    init();
    let f = fixture(0);
    let hidden = machine_with_a_lost_last_workspace(&f);
    let plain = f.workspace.with_file_name("Plain");
    std::fs::create_dir_all(&plain).unwrap();

    // The stand-in dialog answers with the plain folder first, then with the moved workspace.
    let answers = Rc::new(RefCell::new(vec![hidden.clone(), plain]));
    let picker: FolderPicker = Rc::new(move |_, _, done| {
        done(answers.borrow_mut().pop());
        true
    });
    let app = launch_in(&f, "en", platform(picker));
    assert_eq!(app.ui().get_screen(), "welcome");

    click(&app, "Open workspace…");
    assert_eq!(
        app.ui().get_screen(),
        "welcome",
        "a plain folder is not a workspace"
    );
    assert!(
        app.ui()
            .get_notice()
            .starts_with("Cannot open the workspace:"),
        "{}",
        app.ui().get_notice()
    );

    click(&app, "Dismiss");
    assert_eq!(app.ui().get_notice(), "");
    click(&app, "Open workspace…");
    assert_eq!(app.ui().get_screen(), "workspace");
    assert_eq!(app.ui().get_workspace_name(), "Second");
    let known = Engine::known_workspaces(&f.dirs);
    assert!(
        known
            .iter()
            .any(|w| w.name == "Second" && w.path == canonical(&hidden) && w.found),
        "the registry follows the workspace to where it now is: {known:?} (moved to {hidden:?})"
    );
}

/// Clicks the *last* element with this label and role: what is drawn on top (a menu over the
/// window), since it comes last in the tree.
fn click_on_top(app: &App, label: &str) {
    let ui = app.ui();
    let button = ElementQuery::from_root(&ui)
        .match_descendants()
        .match_accessible_role(AccessibleRole::Button)
        .match_predicate({
            let label = label.to_string();
            move |element| element.accessible_label().is_some_and(|l| l == label)
        })
        .find_all()
        .into_iter()
        .next_back()
        .unwrap_or_else(|| panic!("no button labelled {label:?}"));
    button.mock_single_click(PointerEventButton::Left);
}

/// Whether the menu row with this label is enabled.
fn menu_row_enabled(app: &App, label: &str) -> bool {
    let ui = app.ui();
    ElementQuery::from_root(&ui)
        .match_descendants()
        .match_accessible_role(AccessibleRole::Button)
        .match_predicate({
            let label = label.to_string();
            move |element| element.accessible_label().is_some_and(|l| l == label)
        })
        .find_all()
        .into_iter()
        .next_back()
        .and_then(|element| element.accessible_enabled())
        .unwrap_or_else(|| panic!("no menu row labelled {label:?}"))
}

/// The platform's shortcut modifier held while `key` is pressed, as a person would.
fn combo(app: &App, key: &str) {
    let ui = app.ui();
    let window = ui.window();
    let modifier: slint::SharedString = Key::Control.into();
    window.dispatch_event(WindowEvent::KeyPressed {
        text: modifier.clone(),
    });
    window.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: modifier });
}

fn type_keys(app: &App, text: &str) {
    let ui = app.ui();
    for c in text.chars() {
        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: c.to_string().into(),
        });
        ui.window().dispatch_event(WindowEvent::KeyReleased {
            text: c.to_string().into(),
        });
    }
}

/// Puts the keyboard in the text field labelled `label`, with a real click on it.
fn focus_field(app: &App, label: &str) {
    let ui = app.ui();
    let field = ElementHandle::find_by_accessible_label(&ui, label)
        .find(|element| element.accessible_value().is_some())
        .unwrap_or_else(|| panic!("no text field labelled {label:?}"));
    field.mock_single_click(PointerEventButton::Left);
}

fn escape() -> slint::SharedString {
    Key::Escape.into()
}

fn menu_open(app: &App) {
    click(app, "Menu");
    assert!(app.ui().get_menu_open());
}

#[test]
fn the_hamburger_menu_opens_lists_its_sections_and_runs_a_command() {
    init();
    let f = fixture(0);
    let app = launch(&f);
    assert!(!app.ui().get_menu_open());
    menu_open(&app);

    // File is shown first; a click on a row closes the menu and runs the command.
    assert!(menu_row_enabled(&app, "Settings…"));
    assert!(!menu_row_enabled(&app, "Import…"), "no workspace is open");
    click_on_top(&app, "New workspace…");
    assert!(!app.ui().get_menu_open());
    assert_eq!(app.ui().get_dialog(), "new-workspace");

    // Edit and Help are other sections of the same menu.
    click(&app, "Cancel");
    menu_open(&app);
    click_on_top(&app, "Edit");
    assert!(
        !menu_row_enabled(&app, "Select all"),
        "no text field has the keyboard"
    );
    click_on_top(&app, "Help");
    assert!(menu_row_enabled(&app, "About Auroraw"));
}

#[test]
fn importing_is_available_in_the_menu_once_a_workspace_is_open() {
    init();
    let f = fixture(0);
    let app = open(&f);
    app.ui().set_current_task("cull".into());
    menu_open(&app);
    assert!(menu_row_enabled(&app, "Import…"));
    click_on_top(&app, "Import…");
    assert_eq!(app.ui().get_dialog(), "import");
}

#[test]
fn every_command_of_the_menu_is_carried_out_by_the_interface() {
    init();
    let f = fixture(0);
    let picker: FolderPicker = Rc::new(|_, _, _| false);
    let app = launch_in(&f, "en", platform(picker));
    for command in COMMANDS.iter().filter(|c| c.menu) {
        assert!(
            app.launcher.run_command(&app.ui(), command.id),
            "{} is in the menu but the interface does not know it",
            command.id
        );
        app.ui().set_dialog(slint::SharedString::new());
    }
    assert!(!app.launcher.run_command(&app.ui(), "nonsense.command"));
}

#[test]
fn the_shortcuts_reach_the_same_commands_as_the_menu() {
    init();
    let f = fixture(0);
    let app = launch(&f);
    combo(&app, "n");
    assert_eq!(app.ui().get_dialog(), "new-workspace");
    click(&app, "Cancel");

    app.ui().window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::F1.into(),
    });
    assert_eq!(app.ui().get_dialog(), "about");
    assert_eq!(app.ui().get_about_version(), Engine::version());
    click(&app, "Close");
    assert_eq!(app.ui().get_dialog(), "");

    combo(&app, ",");
    assert_eq!(app.ui().get_dialog(), "settings");
}

#[test]
fn escape_closes_the_menu_and_the_dialogs() {
    init();
    let f = fixture(0);
    let app = launch(&f);
    menu_open(&app);
    press(&app, escape().as_str());
    assert!(!app.ui().get_menu_open());

    combo(&app, "n");
    assert_eq!(app.ui().get_dialog(), "new-workspace");
    focus_field(&app, "Name");
    press(&app, escape().as_str());
    assert_eq!(
        app.ui().get_dialog(),
        "",
        "Escape reaches the dialog from its own field"
    );
}

#[test]
fn the_edit_menu_acts_on_the_text_field_that_has_the_keyboard() {
    init();
    let f = fixture(0);
    let app = to_import(open(&f));
    // The import fields are on screen while the menu is used (a dialog would sit over the menu).
    assert_eq!(app.ui().get_dialog(), "import");
    type_into(&app, "Shoot name (optional)", "Patrick");
    focus_field(&app, "Shoot name (optional)");
    let creator = |app: &App| app.ui().get_import_shoot().to_string();

    // A field has the keyboard: Edit's items are usable, and act on it.
    menu_open(&app);
    click_on_top(&app, "Edit");
    assert!(menu_row_enabled(&app, "Select all"));
    click_on_top(&app, "Select all");
    menu_open(&app);
    click_on_top(&app, "Edit");
    click_on_top(&app, "Delete");
    assert_eq!(creator(&app), "", "the selected text was deleted");

    type_keys(&app, "Zed");
    assert_eq!(creator(&app), "Zed");
    app.launcher.run_command(&app.ui(), "edit.select-all");
    app.launcher.run_command(&app.ui(), "edit.cut");
    assert_eq!(creator(&app), "");
    app.launcher.run_command(&app.ui(), "edit.paste");
    assert_eq!(creator(&app), "Zed", "what was cut is pasted back");
    app.launcher.run_command(&app.ui(), "edit.undo");
    assert_ne!(creator(&app), "Zed", "the paste is undone");
    app.launcher.run_command(&app.ui(), "edit.redo");
    assert_eq!(creator(&app), "Zed", "and redone");
}

#[test]
fn the_language_is_chosen_in_settings_and_remembered() {
    init();
    let f = fixture(0);
    let app = launch(&f);
    combo(&app, ",");
    click(&app, "Français");
    assert_eq!(app.ui().get_language(), "fr");
    assert_eq!(
        app.ui().global::<Texts>().invoke_default_workspace_name(),
        "Principal"
    );
    assert_eq!(
        crate::app_settings::AppSettings::load(&f.dirs.data.join("app-settings.json")).language,
        "fr"
    );

    click(&app, "English");
    assert_eq!(
        app.ui().global::<Texts>().invoke_default_workspace_name(),
        "Main"
    );
}

/// Clicks a tab of the top bar.
fn click_tab(app: &App, label: &str) {
    let ui = app.ui();
    let tab = ElementQuery::from_root(&ui)
        .match_descendants()
        .match_accessible_role(AccessibleRole::Tab)
        .match_predicate({
            let label = label.to_string();
            move |element| element.accessible_label().is_some_and(|l| l == label)
        })
        .find_first()
        .unwrap_or_else(|| panic!("no tab labelled {label:?}"));
    tab.mock_single_click(PointerEventButton::Left);
}

/// Adds `folder` as a source through the catalogue panel and waits for its scan to end.
fn add_source_through_the_panel(app: &App, folder: &Path) {
    click(app, "Add a source…");
    assert_eq!(app.ui().get_dialog(), "add-source");
    type_into(app, "Folder", &folder.to_string_lossy());
    click(app, "Add");
}

fn wait_for_the_scan(app: &App) {
    settle("the scan to end", || !app.ui().get_catalogue_busy());
}

#[test]
fn adding_a_folder_from_the_catalogue_panel_scans_it_and_lists_it_without_copying() {
    init();
    let f = fixture(3);
    let app = open(&f);
    assert_eq!(app.ui().get_current_task(), "catalogue");
    assert_eq!(app.ui().get_sources().row_count(), 0);

    add_source_through_the_panel(&app, &f.card);
    assert_eq!(
        app.ui().get_dialog(),
        "",
        "the dialog closes and the scan runs"
    );
    wait_for_the_scan(&app);
    assert_eq!(
        app.ui().get_catalogue_status(),
        "Done: 3 added, 0 restored, 0 already known, 0 not readable."
    );
    let sources = app.ui().get_sources();
    assert_eq!(sources.row_count(), 1);
    let row = sources.row_data(0).unwrap();
    assert_eq!(
        (row.name.as_str(), row.photos, row.online),
        ("Card", 3, true)
    );
    assert!(!f.archive.exists(), "nothing was copied anywhere");

    // The photos are in the grid.
    click_tab(&app, "Cull");
    assert_eq!(app.ui().get_status(), "3 photos");
}

#[test]
fn the_dialog_says_why_a_folder_cannot_be_added() {
    init();
    let f = fixture(2);
    let app = open(&f);
    add_source_through_the_panel(&app, &f.card);
    wait_for_the_scan(&app);

    // Inside a source: already covered. Not a folder, and not absolute: refused with the reason.
    std::fs::create_dir_all(f.card.join("September")).unwrap();
    click(&app, "Add a source…");
    type_into(&app, "Folder", &f.card.join("September").to_string_lossy());
    click(&app, "Add");
    assert!(
        app.ui().get_add_source_error().contains("already part of"),
        "{}",
        app.ui().get_add_source_error()
    );
    assert_eq!(app.ui().get_dialog(), "add-source");

    type_into(&app, "Folder", "photos/relative");
    click(&app, "Add");
    assert!(app.ui().get_add_source_error().contains("absolute"));
    assert_eq!(
        app.ui().get_sources().row_count(),
        1,
        "nothing else was registered"
    );
}

#[test]
fn a_folder_that_contains_a_source_asks_before_merging_it() {
    init();
    let f = fixture(2);
    let parent = f.card.parent().unwrap().join("Photos");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::rename(&f.card, parent.join("Card")).unwrap();
    write_jpeg(&parent.join("top.jpg"), 9);
    let inner = parent.join("Card");

    let app = open(&f);
    add_source_through_the_panel(&app, &inner);
    wait_for_the_scan(&app);

    click(&app, "Add a source…");
    type_into(&app, "Folder", &parent.to_string_lossy());
    click(&app, "Add");
    assert!(
        app.ui()
            .get_add_source_merge_question()
            .contains("\"Card\""),
        "{}",
        app.ui().get_add_source_merge_question()
    );
    assert_eq!(app.ui().get_sources().row_count(), 1, "nothing changed yet");

    click(&app, "Merge and add");
    wait_for_the_scan(&app);
    let sources = app.ui().get_sources();
    assert_eq!(
        sources.row_count(),
        1,
        "the inner source was merged into the new one"
    );
    let row = sources.row_data(0).unwrap();
    assert_eq!((row.name.as_str(), row.photos), ("Photos", 3));
}

#[test]
fn removing_a_source_confirms_with_the_numbers_and_can_be_undone_by_adding_it_again() {
    init();
    let f = fixture(2);
    let app = open(&f);
    add_source_through_the_panel(&app, &f.card);
    wait_for_the_scan(&app);

    // Rate one photo, so that there is work to lose.
    click_tab(&app, "Cull");
    click_cell(&app, 0);
    press(&app, "4");
    settle("the rating", || {
        catalogue(&f).list_by_min_rating(4, None, 10).unwrap().len() == 1
    });
    click_tab(&app, "Catalogue");

    click(&app, "Remove: Card");
    assert_eq!(app.ui().get_dialog(), "remove-source");
    assert_eq!(
        (
            app.ui().get_remove_photos(),
            app.ui().get_remove_worked_on()
        ),
        (2, 1)
    );
    click(&app, "Remove");
    wait_for_the_scan(&app);
    assert_eq!(
        app.ui().get_catalogue_status(),
        "Source \"Card\" removed; 2 photos left the catalogue."
    );
    assert_eq!(app.ui().get_sources().row_count(), 0);
    assert_eq!(catalogue(&f).count_all().unwrap(), 0);
    assert!(
        f.card.join("IMG_0000.jpg").is_file(),
        "the originals are untouched"
    );

    // Adding it again offers to bring the photos back, with their rating.
    add_source_through_the_panel(&app, &f.card);
    settle("the question", || app.ui().get_dialog() == "restore");
    assert_eq!(
        (app.ui().get_restore_count(), app.ui().get_restore_total()),
        (2, 2)
    );
    click(&app, "Restore them");
    wait_for_the_scan(&app);
    assert_eq!(
        app.ui().get_catalogue_status(),
        "Done: 0 added, 2 restored, 0 already known, 0 not readable."
    );
    assert_eq!(
        catalogue(&f).list_by_min_rating(4, None, 10).unwrap().len(),
        1
    );
    assert_eq!(app.ui().get_sources().row_data(0).unwrap().photos, 2);
}

#[test]
fn a_source_can_be_rescanned_to_pick_up_new_photos() {
    init();
    let f = fixture(1);
    let app = open(&f);
    add_source_through_the_panel(&app, &f.card);
    wait_for_the_scan(&app);
    write_jpeg(&f.card.join("IMG_new.jpg"), 42);

    click(&app, "Rescan: Card");
    wait_for_the_scan(&app);
    assert_eq!(
        app.ui().get_catalogue_status(),
        "Done: 1 added, 0 restored, 1 already known, 0 not readable."
    );
    assert_eq!(app.ui().get_sources().row_data(0).unwrap().photos, 2);
}

#[test]
fn the_catalogue_panel_and_its_dialogs_render() {
    init_rendering();
    let f = fixture(6);
    let app = open_in(&f, "fr");
    snapshot(&app, "catalogue-empty");

    click(&app, "Ajouter une source…");
    type_into(&app, "Dossier", &f.card.to_string_lossy());
    snapshot(&app, "add-source-dialog");
    click(&app, "Ajouter");
    wait_for_the_scan(&app);
    snapshot(&app, "catalogue-with-a-source");

    click(&app, "Retirer : Card");
    snapshot(&app, "remove-source-dialog");
}

fn wait_for_the_import(app: &App) {
    settle("the import to end", || app.ui().get_import_finished());
}

#[test]
fn the_import_dialog_says_what_the_destination_is_to_the_catalogue() {
    init();
    let f = fixture(2);
    let app = open(&f);
    let app = to_import(app);
    type_into(
        &app,
        "Import from (card or folder)",
        &f.card.to_string_lossy(),
    );

    // A folder that is not in the catalogue: a copy, unless it is added as a source (the default).
    type_into(&app, "Destination folder", &f.archive.to_string_lossy());
    app.ui().invoke_import_fields_changed();
    assert_eq!(app.ui().get_import_destination_kind(), "not-covered");
    assert!(app.ui().get_import_add_destination());
    assert!(app.ui().get_import_registering());
    assert!(
        app.ui()
            .get_import_destination_note()
            .contains("becomes a source")
    );

    app.ui().set_import_add_destination(false);
    app.ui().invoke_import_fields_changed();
    assert!(!app.ui().get_import_registering());
    assert!(
        app.ui()
            .get_import_destination_note()
            .contains("only copied")
    );

    // A folder inside a source of the catalogue: the photos enter it, whatever the checkbox says.
    app.ui().set_dialog("".into());
    click_tab(&app, "Catalogue");
    let library = f.archive.parent().unwrap().join("Library");
    std::fs::create_dir_all(&library).unwrap();
    add_source_through_the_panel(&app, &library);
    wait_for_the_scan(&app);
    app.launcher.run_command(&app.ui(), "file.import");
    type_into(
        &app,
        "Destination folder",
        &library.join("2026").to_string_lossy(),
    );
    app.ui().invoke_import_fields_changed();
    assert_eq!(app.ui().get_import_destination_kind(), "covered");
    assert!(app.ui().get_import_registering());
    assert!(
        app.ui()
            .get_import_destination_note()
            .contains("part of the source")
    );
}

#[test]
fn a_plain_copy_from_the_dialog_registers_nothing_and_shows_no_photos_to_go_to() {
    init();
    let f = fixture(3);
    let app = to_import(open(&f));
    fill_import_form(&app, &f);
    app.ui().set_import_add_destination(false);
    click(&app, "Import");
    wait_for_the_import(&app);

    assert_eq!(
        app.ui().get_import_status(),
        "All 3 files copied and verified."
    );
    assert!(f.archive.join("IMG_0002.jpg").is_file());
    assert_eq!(
        catalogue(&f).count_all().unwrap(),
        0,
        "nothing entered the catalogue"
    );
    assert!(app.ui().get_sources().row_count() == 0);
    assert!(
        ElementQuery::from_root(&app.ui())
            .match_descendants()
            .match_accessible_role(AccessibleRole::Button)
            .match_predicate(|e| e.accessible_label().is_some_and(|l| l == "Show photos"))
            .find_first()
            .is_none(),
        "a plain copy has no photos to show"
    );
}

#[test]
fn importing_into_a_new_folder_can_add_it_to_the_catalogue_in_the_same_gesture() {
    init();
    let f = fixture(2);
    let app = to_import(open(&f));
    fill_import_form(&app, &f);
    app.ui().invoke_import_fields_changed();
    click(&app, "Import");
    wait_for_the_import(&app);
    settle("the destination to be scanned", || {
        !app.ui().get_catalogue_busy()
    });
    assert_eq!(catalogue(&f).count_all().unwrap(), 2);
    let sources = app.ui().get_sources();
    assert_eq!(sources.row_count(), 1, "the destination became a source");
    assert_eq!(sources.row_data(0).unwrap().name.as_str(), "Archive");
    assert_eq!(sources.row_data(0).unwrap().photos, 2);
}

#[test]
fn a_card_with_camera_folders_offers_to_keep_them() {
    init();
    let f = fixture(0);
    let card = f.card.clone();
    write_jpeg(&card.join("DCIM/100CANON/IMG_0001.JPG"), 1);
    write_jpeg(&card.join("DCIM/101CANON/IMG_0002.JPG"), 2);
    let app = to_import(open(&f));
    fill_import_form(&app, &f);
    app.ui().set_import_add_destination(false);
    app.ui().invoke_import_fields_changed();
    assert!(app.ui().get_import_source_has_folders());
    assert_eq!(app.ui().get_import_source_folders(), "100CANON, 101CANON");

    // The template is proposed; the card's folders are one click away.
    assert_eq!(app.ui().get_import_layout(), "template");
    click(&app, "Keep the card's folders");
    assert_eq!(app.ui().get_import_layout(), "folders");
    click(&app, "Import");
    wait_for_the_import(&app);
    assert!(
        f.archive.join("100CANON/IMG_0001.JPG").is_file(),
        "folders and case kept"
    );
    assert!(f.archive.join("101CANON/IMG_0002.JPG").is_file());
}

/// Cards the stand-in machine has mounted, changed by a test as a person would insert one.
fn cards_platform(cards: &Rc<RefCell<Vec<VolumeInfo>>>) -> Platform {
    let cards = cards.clone();
    Platform {
        pick_folder: no_dialog(),
        volumes: Rc::new(move || cards.borrow().clone()),
    }
}

fn camera_card(name: &str, mount: &Path) -> VolumeInfo {
    VolumeInfo {
        name: name.to_string(),
        mount_point: mount.to_path_buf(),
        has_dcim: true,
    }
}

#[test]
fn a_card_inserted_while_the_application_runs_is_offered_for_import() {
    init();
    let f = fixture(1);
    let cards = Rc::new(RefCell::new(Vec::new()));
    let app = open_with(&f, "en", cards_platform(&cards));
    assert_eq!(app.ui().get_card_banner(), "");

    cards.borrow_mut().push(camera_card("EOS_DIGITAL", &f.card));
    settle("the card to be noticed", || {
        app.ui().get_card_banner() != ""
    });
    assert_eq!(app.ui().get_card_banner(), "Card detected: EOS_DIGITAL");

    // Import… opens the dialog on that card.
    click_on_top(&app, "Import…");
    assert_eq!(app.ui().get_dialog(), "import");
    assert_eq!(app.ui().get_import_source(), f.card.to_string_lossy());
    assert_eq!(app.ui().get_card_banner(), "");

    // The same card does not come back; another inserted later does, and Ignore dismisses it.
    for _ in 0..5 {
        mock_elapsed_time(Duration::from_secs(2));
    }
    assert_eq!(app.ui().get_card_banner(), "");
    let second = f.card.parent().unwrap().join("Second");
    cards.borrow_mut().push(camera_card("NIKON_D850", &second));
    settle("the second card", || app.ui().get_card_banner() != "");
    click_on_top(&app, "Ignore");
    assert_eq!(app.ui().get_card_banner(), "");
}

#[test]
fn a_card_that_was_already_in_when_the_workspace_opened_is_not_announced() {
    init();
    let f = fixture(1);
    let cards = Rc::new(RefCell::new(vec![camera_card("EOS_DIGITAL", &f.card)]));
    let app = open_with(&f, "en", cards_platform(&cards));
    for _ in 0..5 {
        mock_elapsed_time(Duration::from_secs(2));
    }
    assert_eq!(app.ui().get_card_banner(), "");
    // It is listed in the dialog all the same.
    let app = to_import(app);
    assert_eq!(app.ui().get_volumes().row_count(), 1);
}

#[test]
fn the_import_dialog_and_the_card_banner_render() {
    init_rendering();
    let f = fixture(0);
    write_jpeg(&f.card.join("DCIM/100CANON/IMG_0001.JPG"), 1);
    write_jpeg(&f.card.join("DCIM/101CANON/IMG_0002.JPG"), 2);
    let cards = Rc::new(RefCell::new(Vec::new()));
    let app = open_with(&f, "fr", cards_platform(&cards));
    cards.borrow_mut().push(camera_card("EOS_DIGITAL", &f.card));
    settle("the card", || app.ui().get_card_banner() != "");
    snapshot(&app, "card-banner");
    click_on_top(&app, "Importer…");
    type_into(&app, "Dossier de destination", &f.archive.to_string_lossy());
    app.ui().invoke_import_fields_changed();
    snapshot(&app, "import-dialog");
}
