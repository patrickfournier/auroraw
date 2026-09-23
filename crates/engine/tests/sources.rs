// SPDX-License-Identifier: GPL-3.0-or-later
//! Adding, scanning and removing sources as a person does (M1 plan, workflow revision D-090 to
//! D-092): adding a folder copies nothing and builds the sidecars from what the files say, a folder
//! inside a source is already covered, removing a source is recoverable and adding it again offers to
//! bring its photos back with their ratings.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use auroraw_engine::{
    AddPlan, AddSourceRequest, Command, Engine, EngineError, Event, EventReceiver, JobId, Outcome,
};
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::PhotoId;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

struct Setup {
    dir: TempDir,
    engine: Engine,
    events: EventReceiver,
}

fn setup() -> Setup {
    let dir = temp_dir();
    let (engine, events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    Setup {
        dir,
        engine,
        events,
    }
}

fn jpeg(path: &Path, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let img = ImageBuffer::from_fn(32, 24, |x, y| {
        Rgb([(x * 8) as u8 ^ seed, (y * 9) as u8, seed])
    });
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, ImageFormat::Jpeg)
        .unwrap();
}

fn folder_with(setup: &Setup, name: &str, files: &[&str]) -> PathBuf {
    let folder = setup.dir.path().join(name);
    for (i, file) in files.iter().enumerate() {
        jpeg(&folder.join(file), i as u8 + 1);
    }
    folder
}

fn wait_for(events: &EventReceiver, wanted: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left.min(Duration::from_millis(200))) {
            Some(event) if wanted(&event) => return event,
            Some(_) => {}
            None => assert!(Instant::now() < deadline, "the event never came"),
        }
    }
}

fn finished(events: &EventReceiver, job: JobId) -> (usize, usize, usize, usize) {
    match wait_for(
        events,
        |e| matches!(e, Event::IndexFinished { job: j, .. } if *j == job),
    ) {
        Event::IndexFinished {
            added,
            restored,
            known,
            failed,
            ..
        } => (added, restored, known, failed),
        _ => unreachable!(),
    }
}

fn add(setup: &Setup, root: &Path) -> (auroraw_types::SourceId, JobId) {
    let added = setup
        .engine
        .add_source(AddSourceRequest {
            root: root.to_path_buf(),
            name: None,
            merge: false,
        })
        .unwrap();
    (added.source_id, added.job)
}

fn photo_count(setup: &Setup) -> u64 {
    setup.engine.read_catalogue().unwrap().count_all().unwrap()
}

#[test]
fn adding_a_folder_registers_its_photos_and_only_its_photos_without_copying() {
    let s = setup();
    let folder = folder_with(&s, "Trip", &["a.jpg", "sub/b.jpg", "sub/deeper/c.JPG"]);
    std::fs::write(folder.join("README.txt"), b"not a photo").unwrap();
    std::fs::write(folder.join("a.xmp"), b"<x/>").unwrap();

    let (_, job) = add(&s, &folder);
    assert_eq!(finished(&s.events, job), (3, 0, 0, 0));
    assert_eq!(photo_count(&s), 3);
    // Nothing was written into the source (D-018).
    let mut names: Vec<String> = std::fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["README.txt", "a.jpg", "a.xmp", "sub"]);

    let sources = s.engine.sources().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].name, "Trip");
    assert_eq!((sources[0].photos, sources[0].online), (3, true));
}

#[test]
fn scanning_again_adds_nothing_and_finds_new_files_only() {
    let s = setup();
    let folder = folder_with(&s, "Trip", &["a.jpg", "b.jpg"]);
    let (source, job) = add(&s, &folder);
    finished(&s.events, job);

    let Outcome::IndexStarted { job } = s
        .engine
        .submit_and_wait(Command::IndexSource {
            source_id: source,
            merge: vec![],
        })
        .unwrap()
    else {
        panic!("expected IndexStarted");
    };
    assert_eq!(finished(&s.events, job), (0, 0, 2, 0));

    jpeg(&folder.join("c.jpg"), 9);
    let Outcome::IndexStarted { job } = s
        .engine
        .submit_and_wait(Command::IndexSource {
            source_id: source,
            merge: vec![],
        })
        .unwrap()
    else {
        panic!("expected IndexStarted");
    };
    assert_eq!(finished(&s.events, job), (1, 0, 2, 0));
    assert_eq!(photo_count(&s), 3);
}

#[test]
fn a_raw_and_its_jpeg_in_one_folder_are_one_photo_with_two_files() {
    let s = setup();
    let folder = s.dir.path().join("Card");
    std::fs::create_dir_all(&folder).unwrap();
    // Bytes no decoder can read: the photo is still registered, from its name alone.
    std::fs::write(folder.join("IMG_0001.CR2"), b"raw bytes").unwrap();
    jpeg(&folder.join("IMG_0001.JPG"), 3);
    jpeg(&folder.join("IMG_0002.JPG"), 4);

    let (_, job) = add(&s, &folder);
    assert_eq!(finished(&s.events, job), (2, 0, 0, 0));
    let catalogue = s.engine.read_catalogue().unwrap();
    let rows = catalogue.list_recent(None, 10).unwrap();
    assert_eq!(rows.len(), 2);
    let paired = rows.iter().find(|r| r.filename == "IMG_0001.CR2").unwrap();
    let sidecar = s
        .engine
        .workspace()
        .read_photo(&paired.id)
        .unwrap()
        .unwrap()
        .current()
        .unwrap();
    assert_eq!(sidecar.files.len(), 2, "the RAW and its companion JPEG");
}

#[test]
fn real_raw_files_give_their_camera_and_capture_time_to_the_catalogue() {
    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/samples");
    if !samples.join("B13A0732.CR2").is_file() {
        assert!(
            std::env::var_os("AUR_REQUIRE_SAMPLES").is_none(),
            "testdata/samples is required here"
        );
        eprintln!("testdata/samples not found: skipping");
        return;
    }
    let s = setup();
    let folder = s.dir.path().join("Shoot");
    std::fs::create_dir_all(&folder).unwrap();
    for name in ["B13A0732.CR2", "DSC00396.ARW"] {
        std::fs::copy(samples.join(name), folder.join(name)).unwrap();
    }
    let (_, job) = add(&s, &folder);
    assert_eq!(finished(&s.events, job), (2, 0, 0, 0));
    let rows = s
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 10)
        .unwrap();
    let cameras: Vec<String> = rows.iter().filter_map(|r| r.camera.clone()).collect();
    assert!(
        cameras.iter().any(|c| c.contains("5D Mark IV")),
        "{cameras:?}"
    );
    assert!(cameras.iter().any(|c| c.contains("7RM4")), "{cameras:?}");
    assert!(
        rows.iter().any(|r| r.capture_time > 0),
        "a capture time was read: {:?}",
        rows.iter().map(|r| r.capture_time).collect::<Vec<_>>()
    );
}

#[test]
fn a_folder_inside_a_source_is_already_covered_and_a_prefix_is_not_a_parent() {
    let s = setup();
    let folder = folder_with(&s, "photos/2026", &["a.jpg"]);
    let (_, job) = add(&s, &folder);
    finished(&s.events, job);

    let inside = folder.join("September");
    std::fs::create_dir_all(&inside).unwrap();
    assert!(matches!(
        s.engine.plan_add_source(&inside).unwrap(),
        AddPlan::InsideExisting(_)
    ));
    let error = s
        .engine
        .add_source(AddSourceRequest {
            root: inside.clone(),
            name: None,
            merge: false,
        })
        .unwrap_err();
    assert!(matches!(error, EngineError::AlreadyCovered(_)), "{error}");
    assert_eq!(
        s.engine.sources().unwrap().len(),
        1,
        "nothing was registered"
    );

    let (covering, relative) = s.engine.covering_source(&inside).unwrap().unwrap();
    assert_eq!(covering.name, "2026");
    assert_eq!(relative, PathBuf::from("September"));

    // `2026-old` shares a name prefix with `2026`, and is not inside it.
    let sibling = s.dir.path().join("photos/2026-old");
    std::fs::create_dir_all(&sibling).unwrap();
    assert_eq!(s.engine.plan_add_source(&sibling).unwrap(), AddPlan::Free);
    assert!(s.engine.covering_source(&sibling).unwrap().is_none());

    // The parent contains the source.
    let parent = s.dir.path().join("photos");
    let AddPlan::ContainsExisting(inner) = s.engine.plan_add_source(&parent).unwrap() else {
        panic!("the parent contains a source");
    };
    assert_eq!(inner.len(), 1);
    assert!(matches!(
        s.engine
            .add_source(AddSourceRequest {
                root: parent,
                name: None,
                merge: false
            })
            .unwrap_err(),
        EngineError::ContainsSources(_)
    ));
}

#[test]
fn a_scan_never_descends_into_another_sources_folder() {
    let s = setup();
    let parent = folder_with(&s, "photos", &["top.jpg", "inner/one.jpg", "inner/two.jpg"]);
    let inner = parent.join("inner");
    // The inner folder was registered first; the parent is registered through the command (the
    // engine's own add_source would refuse it).
    let Outcome::SourceAdded(inner_id) = s
        .engine
        .submit_and_wait(Command::AddSource {
            name: "inner".into(),
            root: inner,
            kind: "local-folder".into(),
        })
        .unwrap()
    else {
        panic!();
    };
    let Outcome::IndexStarted { job } = s
        .engine
        .submit_and_wait(Command::IndexSource {
            source_id: inner_id,
            merge: vec![],
        })
        .unwrap()
    else {
        panic!();
    };
    finished(&s.events, job);
    let Outcome::SourceAdded(parent_id) = s
        .engine
        .submit_and_wait(Command::AddSource {
            name: "photos".into(),
            root: parent,
            kind: "local-folder".into(),
        })
        .unwrap()
    else {
        panic!();
    };
    let Outcome::IndexStarted { job } = s
        .engine
        .submit_and_wait(Command::IndexSource {
            source_id: parent_id,
            merge: vec![],
        })
        .unwrap()
    else {
        panic!();
    };
    assert_eq!(
        finished(&s.events, job),
        (1, 0, 0, 0),
        "only top.jpg is the parent's"
    );
    assert_eq!(photo_count(&s), 3, "each photo once");
}

fn rate(s: &Setup, photo: PhotoId, stars: u8) {
    s.engine
        .submit_and_wait(Command::SetRating {
            photo_id: photo,
            rating: stars,
        })
        .unwrap();
}

fn remove(s: &Setup, source: auroraw_types::SourceId) -> (usize, usize) {
    let Outcome::RemoveStarted { job } = s
        .engine
        .submit_and_wait(Command::RemoveSource { source_id: source })
        .unwrap()
    else {
        panic!("expected RemoveStarted");
    };
    match wait_for(
        &s.events,
        |e| matches!(e, Event::SourceRemoved { job: j, .. } if *j == job),
    ) {
        Event::SourceRemoved { removed, kept, .. } => (removed, kept),
        _ => unreachable!(),
    }
}

fn removed_sidecars(s: &Setup) -> usize {
    s.engine.workspace().removed_photos().len()
}

#[test]
fn removing_a_source_takes_its_photos_out_recoverably_and_touches_no_original() {
    let s = setup();
    let folder = folder_with(&s, "Trip", &["a.jpg", "b.jpg", "c.jpg"]);
    let (source, job) = add(&s, &folder);
    finished(&s.events, job);
    let photo = s
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 1)
        .unwrap()[0]
        .id;
    rate(&s, photo, 4);

    let counts = s.engine.source_counts(source).unwrap();
    assert_eq!((counts.photos, counts.worked_on), (3, 1));

    assert_eq!(remove(&s, source), (3, 0));
    assert_eq!(photo_count(&s), 0);
    assert!(s.engine.sources().unwrap().is_empty());
    assert_eq!(removed_sidecars(&s), 3, "kept in removed/, not deleted");
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        assert!(folder.join(name).is_file(), "{name} is untouched");
    }
}

#[test]
fn adding_a_removed_source_again_offers_its_photos_back_with_their_ratings() {
    let s = setup();
    let folder = folder_with(&s, "Trip", &["a.jpg", "b.jpg"]);
    let (source, job) = add(&s, &folder);
    finished(&s.events, job);
    let photo = s
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 1)
        .unwrap()[0]
        .id;
    rate(&s, photo, 5);
    remove(&s, source);

    // Adding it again: the job stops to ask, because both files match removed photos.
    let (_, job) = add(&s, &folder);
    let Event::IndexPlanned {
        new_files,
        restorable,
        ..
    } = wait_for(&s.events, |e| matches!(e, Event::IndexPlanned { .. }))
    else {
        unreachable!()
    };
    assert_eq!((new_files, restorable), (2, 2));
    s.engine
        .submit_and_wait(Command::ContinueIndex {
            job_id: job,
            restore: true,
        })
        .unwrap();
    assert_eq!(finished(&s.events, job), (0, 2, 0, 0));

    // The same photos, with their work: the rating is back on the same identifier.
    let catalogue = s.engine.read_catalogue().unwrap();
    assert_eq!(catalogue.photo(&photo).unwrap().unwrap().rating, 5);
    assert_eq!(catalogue.count_all().unwrap(), 2);
    assert_eq!(removed_sidecars(&s), 0, "not offered again");
    let sources = s.engine.sources().unwrap();
    assert_eq!(sources[0].photos, 2, "they are in the new source");
}

#[test]
fn a_removed_source_added_again_can_start_over_with_new_photos() {
    let s = setup();
    let folder = folder_with(&s, "Trip", &["a.jpg", "b.jpg"]);
    let (source, job) = add(&s, &folder);
    finished(&s.events, job);
    let old = s
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 1)
        .unwrap()[0]
        .id;
    rate(&s, old, 3);
    remove(&s, source);

    let (_, job) = add(&s, &folder);
    wait_for(&s.events, |e| matches!(e, Event::IndexPlanned { .. }));
    s.engine
        .submit_and_wait(Command::ContinueIndex {
            job_id: job,
            restore: false,
        })
        .unwrap();
    assert_eq!(finished(&s.events, job), (2, 0, 0, 0));
    let catalogue = s.engine.read_catalogue().unwrap();
    assert!(
        catalogue.photo(&old).unwrap().is_none(),
        "new photos, new identifiers"
    );
    assert_eq!(
        removed_sidecars(&s),
        2,
        "the old ones are still recoverable"
    );
}

#[test]
fn adding_a_folder_that_contains_sources_merges_them_keeping_their_photos_and_ratings() {
    let s = setup();
    let parent = folder_with(&s, "photos", &["top.jpg", "2026/a.jpg", "2026/b.jpg"]);
    let inner = parent.join("2026");
    let (inner_id, job) = add(&s, &inner);
    finished(&s.events, job);
    let rated = s
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 1)
        .unwrap()[0]
        .id;
    rate(&s, rated, 5);

    // Without the merge the parent is refused; with it, the inner source's photos are the parent's.
    let refused = s.engine.add_source(AddSourceRequest {
        root: parent.clone(),
        name: None,
        merge: false,
    });
    assert!(matches!(refused, Err(EngineError::ContainsSources(_))));
    let added = s
        .engine
        .add_source(AddSourceRequest {
            root: parent.clone(),
            name: Some("All photos".into()),
            merge: true,
        })
        .unwrap();
    // Only top.jpg is new: the two in 2026/ were the inner source's.
    assert_eq!(finished(&s.events, added.job), (1, 0, 2, 0));

    let sources = s.engine.sources().unwrap();
    assert_eq!(sources.len(), 1, "the inner source is gone: {sources:?}");
    assert_eq!(sources[0].name, "All photos");
    assert_eq!(sources[0].photos, 3, "each photo once, in the parent");
    assert_ne!(sources[0].id, inner_id);
    let catalogue = s.engine.read_catalogue().unwrap();
    let row = catalogue.photo(&rated).unwrap().unwrap();
    assert_eq!(row.rating, 5, "its rating came with it");
    assert_eq!(row.source_id, Some(sources[0].id));
    assert!(
        row.path.as_deref().unwrap().starts_with("2026/"),
        "{:?}",
        row.path
    );
    let sidecar = s
        .engine
        .workspace()
        .read_photo(&rated)
        .unwrap()
        .unwrap()
        .current()
        .unwrap();
    assert_eq!(sidecar.files[0].locations.len(), 1);
    assert_eq!(sidecar.files[0].locations[0].source, sources[0].id);

    // A scan again finds nothing new.
    let Outcome::IndexStarted { job } = s
        .engine
        .submit_and_wait(Command::IndexSource {
            source_id: sources[0].id,
            merge: vec![],
        })
        .unwrap()
    else {
        panic!();
    };
    assert_eq!(finished(&s.events, job), (0, 0, 3, 0));
}
