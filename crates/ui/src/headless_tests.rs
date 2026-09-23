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

use auroraw_engine::{Command, Engine};
use i_slint_backend_testing::{
    AccessibleRole, ElementHandle, ElementQuery, TestingBackend, TestingBackendOptions,
    init_no_event_loop, mock_elapsed_time,
};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model};

use crate::generated::Texts;
use crate::{FolderPicker, LocalPaths, Shell, build, start_directory};

/// A workspace, a folder of photos standing in for a card, and where everything local goes.
struct Fixture {
    _dir: auroraw_testkit::TempDir,
    workspace: PathBuf,
    card: PathBuf,
    archive: PathBuf,
    paths: LocalPaths,
}

fn write_jpeg(path: &Path, seed: u8) {
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
    let local = dir.path().join("Workspace/.auroraw");
    let paths = LocalPaths {
        previews: local.join("previews.db"),
        settings: local.join("settings.json"),
        import_state: local.join("import"),
    };
    Fixture {
        workspace: dir.path().join("Workspace"),
        archive: dir.path().join("Archive"),
        card,
        paths,
        _dir: dir,
    }
}

/// Opens (creating on first call) the fixture's workspace and builds the shell over it, in English:
/// a test's own text checks must not depend on the machine's language.
fn open(fixture: &Fixture) -> (Shell, Engine) {
    open_in(fixture, "en")
}

/// As [`open`], in `language`. The bundled translations only exist once a window has been built,
/// so the language is chosen after it.
fn open_in(fixture: &Fixture, language: &str) -> (Shell, Engine) {
    open_with(fixture, language, no_dialog())
}

/// A folder picker that must never be reached: a test that does not browse.
fn no_dialog() -> FolderPicker {
    Rc::new(|_, _, _| panic!("no folder dialog was expected"))
}

/// As [`open_in`], with `pick_folder` standing in for the system's folder dialog.
fn open_with(fixture: &Fixture, language: &str, pick_folder: FolderPicker) -> (Shell, Engine) {
    let catalogue = fixture.workspace.join(".auroraw/catalogue.sqlite");
    std::fs::create_dir_all(catalogue.parent().unwrap()).unwrap();
    let (engine, events) = if catalogue.exists() {
        Engine::open(&fixture.workspace, &catalogue).unwrap()
    } else {
        Engine::create(&fixture.workspace, &catalogue, "Test").unwrap()
    };
    let shell = build(engine.clone(), events, &fixture.paths, pick_folder).unwrap();
    slint::select_bundled_translation(language).unwrap();
    // Elements under a condition only exist once the window has been laid out.
    shell.ui.show().unwrap();
    (shell, engine)
}

/// The testing backend for this thread.
fn init() {
    init_no_event_loop();
}

/// Types `text` into the field labelled `label`, the way an assistive technology sets a value.
fn type_into(shell: &Shell, label: &str, text: &str) {
    let field = ElementHandle::find_by_accessible_label(&shell.ui, label)
        .find(|element| element.accessible_value().is_some())
        .unwrap_or_else(|| panic!("no text field labelled {label:?}"));
    field.set_accessible_value(text);
}

/// Clicks the button labelled `label`: a real pointer press and release at its centre.
fn click(shell: &Shell, label: &str) {
    let button = ElementQuery::from_root(&shell.ui)
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

fn fill_import_form(shell: &Shell, f: &Fixture) {
    type_into(
        shell,
        "Import from (card or folder)",
        &f.card.to_string_lossy(),
    );
    type_into(shell, "Archive folder", &f.archive.to_string_lossy());
}

#[test]
fn an_empty_library_opens_on_the_import_task_and_one_with_photos_on_the_grid() {
    init();
    let f = fixture(2);
    let (shell, engine) = open(&f);
    assert_eq!(shell.ui.get_current_task(), "import");

    let root = f.card.clone();
    let Ok(auroraw_engine::Outcome::SourceAdded(source)) =
        engine.submit_and_wait(Command::AddSource {
            name: "Card".into(),
            root,
            kind: "local-folder".into(),
        })
    else {
        panic!("expected SourceAdded");
    };
    let auroraw_engine::Outcome::Scanned { new, .. } = engine
        .submit_and_wait(Command::ScanSource { source_id: source })
        .unwrap()
    else {
        panic!("expected Scanned");
    };
    engine
        .submit_and_wait(Command::AddNewPhotos {
            source_id: source,
            paths: new,
        })
        .unwrap();
    drop(shell);
    let (shell, _engine) = open(&f);
    assert_eq!(shell.ui.get_current_task(), "cull");
    assert_eq!(shell.ui.get_status(), "2 photos");
}

#[test]
fn filling_the_form_and_clicking_import_copies_verifies_and_shows_the_photos() {
    init();
    let f = fixture(3);
    let (shell, _engine) = open(&f);
    fill_import_form(&shell, &f);
    type_into(&shell, "Creator", "Patrick Fournier");
    assert_eq!(shell.ui.get_import_archive(), f.archive.to_string_lossy());

    click(&shell, "Import");
    assert!(shell.ui.get_importing(), "the import started");
    settle("the import to finish", || shell.ui.get_import_finished());

    assert!(!shell.ui.get_importing());
    assert_eq!(
        shell.ui.get_import_status(),
        "All 3 files copied and verified."
    );
    for n in 0..3 {
        assert!(
            f.archive.join(format!("IMG_000{n}.jpg")).is_file(),
            "IMG_000{n}.jpg reached the archive (no date folders: the photos carry no capture time)"
        );
    }
    assert!(
        std::fs::read_dir(&f.paths.import_state)
            .unwrap()
            .next()
            .is_none(),
        "a clean import leaves no state to resume"
    );

    // "Show photos" switches to the grid with what was just imported.
    click(&shell, "Show photos");
    assert_eq!(shell.ui.get_current_task(), "cull");
    assert_eq!(shell.ui.get_status(), "3 photos");
    assert_eq!(shell.ui.get_rows().row_count(), 1);

    // Thumbnails arrive from the background workers and fill the cells.
    settle("the thumbnails", || {
        let row = shell.ui.get_rows().row_data(0).unwrap();
        (0..3).all(|i| row.cells.row_data(i).unwrap().ready)
    });
}

#[test]
fn a_second_import_of_the_same_card_says_so_and_copies_nothing() {
    init();
    let f = fixture(2);
    let (shell, _engine) = open(&f);
    fill_import_form(&shell, &f);
    click(&shell, "Import");
    settle("the first import", || shell.ui.get_import_finished());

    click(&shell, "Import");
    // The flag is reset by the click and set again when the second job ends.
    settle("the second import", || {
        !shell.ui.get_importing() && shell.ui.get_import_finished()
    });
    assert_eq!(
        shell.ui.get_import_status(),
        "0 copied, 2 already in the library, 0 failed. Run it again to retry."
    );
}

#[test]
fn an_import_that_cannot_start_says_why_and_starts_nothing() {
    init();
    let f = fixture(1);
    let (shell, _engine) = open(&f);

    click(&shell, "Import");
    assert!(!shell.ui.get_importing());
    assert!(
        shell
            .ui
            .get_import_status()
            .starts_with("Cannot start the import:"),
        "{}",
        shell.ui.get_import_status()
    );

    type_into(&shell, "Import from (card or folder)", "/nowhere/at/all");
    type_into(&shell, "Archive folder", &f.archive.to_string_lossy());
    click(&shell, "Import");
    assert!(!shell.ui.get_importing());
    assert!(shell.ui.get_import_status().contains("is not a folder"));
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
        let (shell, _engine) = open(&f);
        fill_import_form(&shell, &f);
        type_into(&shell, "Creator", "Patrick Fournier");
        type_into(&shell, "Copyright", "© Patrick Fournier");
        click(&shell, "Import");
        settle("the import", || shell.ui.get_import_finished());
    }
    let (shell, _engine) = open(&f);
    assert_eq!(shell.ui.get_import_archive(), f.archive.to_string_lossy());
    assert_eq!(shell.ui.get_import_source(), f.card.to_string_lossy());
    assert_eq!(shell.ui.get_import_creator(), "Patrick Fournier");
    assert_eq!(shell.ui.get_import_rights(), "© Patrick Fournier");
}

#[test]
fn rating_from_the_keyboard_reaches_the_catalogue() {
    init();
    let f = fixture(2);
    let (shell, engine) = open(&f);
    fill_import_form(&shell, &f);
    click(&shell, "Import");
    settle("the import", || shell.ui.get_import_finished());
    click(&shell, "Show photos");

    click_cell(&shell, 1);
    press(&shell, "4");
    assert_eq!(
        shell
            .ui
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
        engine
            .read_catalogue()
            .unwrap()
            .list_by_min_rating(4, None, 10)
            .unwrap()
            .len()
            == 1
    });
}

/// A key press and release, as the window's own event loop would deliver them.
fn press(shell: &Shell, text: &str) {
    let window = shell.ui.window();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: text.into() });
}

/// Clicks the n-th cell of the grid's first row.
fn click_cell(shell: &Shell, n: usize) {
    // A cell is 160 x 120 logical pixels with 4 between them, below the two 36 px bars.
    let x = 4.0 + n as f32 * 164.0 + 80.0;
    let y = 72.0 + 60.0;
    let position = slint::LogicalPosition::new(x, y);
    shell
        .ui
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    shell
        .ui
        .window()
        .dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
    shell
        .ui
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
    let (shell, _engine) = open(&f);
    let texts = shell.ui.global::<Texts>();
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
fn snapshot(shell: &Shell, name: &str) {
    shell
        .ui
        .window()
        .set_size(slint::PhysicalSize::new(1400, 900));
    shell.ui.show().unwrap();
    let picture = shell.ui.window().take_snapshot().expect("a picture");
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
    let (shell, _engine) = open_in(&f, "fr");
    snapshot(&shell, "import-empty");

    // The labels are French here, so the fields are set directly rather than found by label.
    shell
        .ui
        .set_import_source(f.card.to_string_lossy().as_ref().into());
    shell
        .ui
        .set_import_archive(f.archive.to_string_lossy().as_ref().into());
    click(&shell, "Import");
    settle("the import", || shell.ui.get_import_finished());
    snapshot(&shell, "import-done");

    click(&shell, "Voir les photos");
    settle("the thumbnails", || {
        let row = shell.ui.get_rows().row_data(0).unwrap();
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
    let (shell, _engine) = open_with(&f, "en", stand_in(&dialogs));

    // An empty field opens the system's default place; the answer fills the field.
    dialogs.borrow_mut().answer = Some(f.archive.clone());
    click(&shell, "Browse for: Archive folder");
    assert_eq!(shell.ui.get_import_archive(), f.archive.to_string_lossy());
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
    assert_eq!(shell.ui.get_import_source(), f.card.to_string_lossy());
    assert_eq!(
        dialogs.borrow().asked[1],
        (
            "Choose the card or folder to import from".to_string(),
            Some(f.workspace.clone())
        )
    );

    dialogs.borrow_mut().answer = Some(f.workspace.clone());
    click(&shell, "Browse for: Backup folder (optional)");
    assert_eq!(shell.ui.get_import_backup(), f.workspace.to_string_lossy());
    assert_eq!(dialogs.borrow().asked[2].0, "Choose the backup folder");
}

#[test]
fn cancelling_the_dialog_leaves_the_field_alone() {
    init();
    let f = fixture(0);
    let dialogs = Rc::new(RefCell::new(Dialogs::default()));
    let (shell, _engine) = open_with(&f, "en", stand_in(&dialogs));
    type_into(&shell, "Archive folder", "/kept/as/typed");

    dialogs.borrow_mut().answer = None;
    click(&shell, "Browse for: Archive folder");
    assert_eq!(shell.ui.get_import_archive(), "/kept/as/typed");
    assert!(!shell.ui.get_picking(), "the buttons are usable again");
}

#[test]
fn while_a_dialog_is_open_another_one_cannot_be_started() {
    init();
    let f = fixture(0);
    let dialogs = Rc::new(RefCell::new(Dialogs {
        hold: true,
        ..Dialogs::default()
    }));
    let (shell, _engine) = open_with(&f, "en", stand_in(&dialogs));

    click(&shell, "Browse for: Archive folder");
    assert!(shell.ui.get_picking());
    click(&shell, "Browse for: Import from (card or folder)");
    assert_eq!(
        dialogs.borrow().asked.len(),
        1,
        "the second click did nothing"
    );

    let done = dialogs.borrow_mut().held.take().unwrap();
    done(Some(f.archive.clone()));
    assert!(!shell.ui.get_picking());
    assert_eq!(shell.ui.get_import_archive(), f.archive.to_string_lossy());
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
