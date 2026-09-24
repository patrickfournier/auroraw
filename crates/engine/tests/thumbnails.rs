// SPDX-License-Identifier: GPL-3.0-or-later
//! The background thumbnail service (WP8): generates and caches a thumbnail on first request,
//! serves it from the cache on the next one, and never blocks the caller.

use std::time::{Duration, Instant};

use auroraw_engine::{Command, Engine, EventReceiver, Outcome};
use auroraw_types::PhotoId;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

fn new_engine() -> (Engine, EventReceiver, auroraw_testkit::TempDir) {
    let dir = auroraw_testkit::temp_dir();
    let (engine, events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    (engine, events, dir)
}

fn write_test_jpeg(path: &std::path::Path) {
    let img = ImageBuffer::from_fn(64, 48, |x, y| Rgb([(x * 4) as u8, (y * 5) as u8, 128]));
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, ImageFormat::Jpeg)
        .unwrap();
}

/// Registers `root` as a source, scans it and confirms every new file it finds, returning the
/// first photo's identifier.
fn import_one_photo(engine: &Engine, root: &std::path::Path) -> PhotoId {
    let Outcome::SourceAdded(source_id) = engine
        .submit_and_wait(Command::AddSource {
            name: "Test source".into(),
            root: root.to_path_buf(),
            kind: auroraw_sources::filesystem::LOCAL_FOLDER.into(),
        })
        .unwrap()
    else {
        panic!("expected SourceAdded");
    };
    let Outcome::Scanned { new, .. } = engine
        .submit_and_wait(Command::ScanSource { source_id })
        .unwrap()
    else {
        panic!("expected Scanned");
    };
    let Outcome::PhotosAdded(added) = engine
        .submit_and_wait(Command::AddNewPhotos {
            source_id,
            paths: new,
        })
        .unwrap()
    else {
        panic!("expected PhotosAdded");
    };
    added[0]
}

fn poll_until(
    service: &auroraw_engine::ThumbnailService,
    id: PhotoId,
    timeout: Duration,
) -> auroraw_imaging::Thumbnail {
    let deadline = Instant::now() + timeout;
    loop {
        for (found, thumbnail) in service.poll() {
            if found == id {
                return thumbnail;
            }
        }
        assert!(Instant::now() < deadline, "thumbnail never arrived");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_requested_thumbnail_is_generated_and_then_served_from_the_cache() {
    let (engine, _events, dir) = new_engine();
    let source_root = dir.path().join("Card");
    std::fs::create_dir_all(&source_root).unwrap();
    write_test_jpeg(&source_root.join("a.jpg"));
    let photo_id = import_one_photo(&engine, &source_root);

    let previews_path = dir.path().join("previews.db");
    let service = engine.start_thumbnails(&previews_path, 2).unwrap();

    assert!(
        service.poll().is_empty(),
        "nothing was requested yet, so nothing should be pending"
    );
    service.request(photo_id);
    let thumbnail = poll_until(&service, photo_id, Duration::from_secs(5));
    assert_eq!((thumbnail.width, thumbnail.height), (64, 48));
    assert!(!thumbnail.jpeg.is_empty());

    // A fresh service (a second worker pool, as a later application launch would start) finds
    // the same thumbnail already cached: no decode needed.
    let previews = auroraw_imaging::PreviewsDb::open(&previews_path).unwrap();
    let cached = previews.get(&photo_id).unwrap().unwrap();
    assert_eq!(cached.jpeg, thumbnail.jpeg);
}

#[test]
fn requesting_the_same_photo_twice_before_it_arrives_is_not_duplicated_work() {
    let (engine, _events, dir) = new_engine();
    let source_root = dir.path().join("Card");
    std::fs::create_dir_all(&source_root).unwrap();
    write_test_jpeg(&source_root.join("a.jpg"));
    let photo_id = import_one_photo(&engine, &source_root);

    let service = engine
        .start_thumbnails(&dir.path().join("previews.db"), 1)
        .unwrap();
    service.request(photo_id);
    service.request(photo_id);
    service.request(photo_id);

    let mut seen = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        seen += service.poll().len();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(seen, 1, "three requests for the same photo deliver it once");
}

#[test]
fn a_photo_with_no_decodable_file_is_never_delivered_but_never_blocks_either() {
    let (engine, _events, dir) = new_engine();
    let source_root = dir.path().join("Card");
    std::fs::create_dir_all(&source_root).unwrap();
    std::fs::write(source_root.join("a.jpg"), b"not a real jpeg").unwrap();
    let photo_id = import_one_photo(&engine, &source_root);

    let service = engine
        .start_thumbnails(&dir.path().join("previews.db"), 1)
        .unwrap();
    service.request(photo_id);
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        service.poll().is_empty(),
        "an undecodable file yields nothing, not a crash"
    );
}

#[test]
fn a_photo_no_thumbnail_can_be_made_from_is_reported_not_silently_missing() {
    let (engine, _events, dir) = new_engine();
    let source_root = dir.path().join("Card");
    std::fs::create_dir_all(&source_root).unwrap();
    // A file with a photo's extension but nothing decodable in it, the same clean failure two of
    // the real sample RAW files give (no embedded preview `rawler` can read).
    std::fs::write(source_root.join("broken.jpg"), b"not an image").unwrap();
    let id = import_one_photo(&engine, &source_root);

    let service = engine
        .start_thumbnails(&dir.path().join("previews.db"), 1)
        .unwrap();
    service.request(id);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(service.poll().is_empty());
        if service.poll_failed().contains(&id) {
            break;
        }
        assert!(Instant::now() < deadline, "the failure was never reported");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_warmed_thumbnail_is_cached_for_later_and_never_delivered() {
    let (engine, _events, dir) = new_engine();
    let source_root = dir.path().join("Card");
    std::fs::create_dir_all(&source_root).unwrap();
    write_test_jpeg(&source_root.join("a.jpg"));
    let photo_id = import_one_photo(&engine, &source_root);

    let previews_path = dir.path().join("previews.db");
    let service = engine.start_thumbnails(&previews_path, 2).unwrap();
    let previews = auroraw_imaging::PreviewsDb::open(&previews_path).unwrap();
    assert!(previews.get(&photo_id).unwrap().is_none());

    service.warm(photo_id);
    service.warm(photo_id);
    let deadline = Instant::now() + Duration::from_secs(5);
    while previews.get(&photo_id).unwrap().is_none() {
        assert!(Instant::now() < deadline, "the thumbnail was never made");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(previews.count().unwrap(), 1);
    assert!(
        service.poll().is_empty(),
        "nobody asked for it, so nothing arrives"
    );
    assert!(service.poll_failed().is_empty());

    // Asking for it afterwards is served from what was made ahead.
    service.request(photo_id);
    let thumbnail = poll_until(&service, photo_id, Duration::from_secs(5));
    assert_eq!((thumbnail.width, thumbnail.height), (64, 48));
}

#[test]
fn a_warmed_photo_that_cannot_be_made_does_not_stop_the_others() {
    let (engine, _events, dir) = new_engine();
    let source_root = dir.path().join("Card");
    std::fs::create_dir_all(&source_root).unwrap();
    write_test_jpeg(&source_root.join("a.jpg"));
    let photo_id = import_one_photo(&engine, &source_root);

    let service = engine
        .start_thumbnails(&dir.path().join("previews.db"), 1)
        .unwrap();
    service.warm(PhotoId::random());
    service.warm(photo_id);
    let previews = auroraw_imaging::PreviewsDb::open(&dir.path().join("previews.db")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while previews.get(&photo_id).unwrap().is_none() {
        assert!(Instant::now() < deadline, "the thumbnail was never made");
        std::thread::sleep(Duration::from_millis(5));
    }
}
