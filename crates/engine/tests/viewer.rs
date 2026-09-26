// SPDX-License-Identifier: GPL-3.0-or-later
//! The image view's service (D-100): the photo asked for is delivered upright at its size, a picture that is
//! kept is answered at once (even if the original went away), a missing original is reported as failed, the
//! photos said to be next are made ahead, and no more than the cache's capacity is kept.

use std::path::Path;
use std::time::{Duration, Instant};

use auroraw_engine::{
    AddSourceRequest, Engine, Event, EventReceiver, PREVIEW_CACHE, PreviewService,
};
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::PhotoId;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

struct Setup {
    dir: TempDir,
    engine: Engine,
    photos: Vec<PhotoId>,
    folder: std::path::PathBuf,
}

fn jpeg(path: &Path, seed: u8, width: u32, height: u32) {
    let img = ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([(x % 251) as u8 ^ seed, (y % 241) as u8, seed])
    });
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, ImageFormat::Jpeg)
        .unwrap();
}

fn wait_for(events: &EventReceiver, wanted: impl Fn(&Event) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(event) if wanted(&event) => return,
            Some(_) => {}
            None => assert!(Instant::now() < deadline, "the event never came"),
        }
    }
}

/// A catalogue of `count` 300 x 200 photos in one folder.
fn setup(count: usize) -> Setup {
    let dir = temp_dir();
    let folder = dir.path().join("Trip");
    std::fs::create_dir_all(&folder).unwrap();
    for i in 0..count {
        jpeg(&folder.join(format!("p{i:04}.jpg")), i as u8, 300, 200);
    }
    let (engine, events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    let added = engine
        .add_source(AddSourceRequest {
            root: folder.clone(),
            name: None,
            merge: false,
        })
        .unwrap();
    wait_for(
        &events,
        |e| matches!(e, Event::IndexFinished { job, .. } if *job == added.job),
    );
    let photos = engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 10_000)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    Setup {
        dir,
        engine,
        photos,
        folder,
    }
}

/// Asks for `id` and waits for the answer: the JPEG's size, or `None` when it failed.
fn ask(service: &PreviewService, id: PhotoId) -> Option<(u32, u32)> {
    service.request(id);
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Some((_, image)) = service.poll().into_iter().find(|(got, _)| *got == id) {
            return Some((image.width, image.height));
        }
        if service.poll_failed().contains(&id) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("no answer for {id}");
}

fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !condition() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_photo_is_delivered_at_its_size_and_upright_from_its_sidecar() {
    let s = setup(2);
    let service = s.engine.start_previews(2).unwrap();
    assert_eq!(ask(&service, s.photos[0]), Some((300, 200)));

    // The sidecar's orientation turns it: a portrait shot, stored sideways in the file.
    let mut sidecar = s
        .engine
        .workspace()
        .read_photo(&s.photos[1])
        .unwrap()
        .unwrap()
        .current()
        .unwrap();
    sidecar.meta.original.orientation = Some(6);
    s.engine.workspace().write_photo(&sidecar).unwrap();
    assert_eq!(ask(&service, s.photos[1]), Some((200, 300)));
}

#[test]
fn a_picture_that_is_kept_is_answered_at_once_and_a_missing_original_is_reported() {
    let s = setup(3);
    let service = s.engine.start_previews(2).unwrap();
    assert!(ask(&service, s.photos[0]).is_some());
    assert!(service.is_ready(&s.photos[0]));

    // Every original goes away: what was made is still shown, what was not is reported.
    std::fs::remove_dir_all(&s.folder).unwrap();
    service.request(s.photos[0]);
    let answered = service.poll();
    assert_eq!(
        answered.len(),
        1,
        "answered without a worker: already in the inbox"
    );
    assert_eq!(ask(&service, s.photos[1]), None);
    assert!(!service.is_ready(&s.photos[1]));
    drop(s.dir);
}

#[test]
fn the_photos_said_to_be_next_are_made_ahead_and_the_previous_list_is_forgotten() {
    let s = setup(6);
    let service = s.engine.start_previews(2).unwrap();
    service.prefetch(&s.photos[1..4]);
    wait_until("the next photos were made", || {
        s.photos[1..4].iter().all(|p| service.is_ready(p))
    });
    assert!(!service.is_ready(&s.photos[5]), "not asked for, not made");
    // Nothing was asked for, so nothing is delivered.
    assert!(service.poll().is_empty());
    // Asked for now, they arrive without waiting for a worker.
    service.request(s.photos[2]);
    assert_eq!(service.poll().len(), 1);
}

#[test]
fn no_more_than_the_capacity_is_kept_and_the_latest_stay() {
    let count = PREVIEW_CACHE + 4;
    let s = setup(count);
    let service = s.engine.start_previews(2).unwrap();
    for id in &s.photos {
        assert!(ask(&service, *id).is_some());
    }
    assert_eq!(service.kept(), PREVIEW_CACHE);
    assert!(
        service.is_ready(s.photos.last().unwrap()),
        "the newest stays"
    );
    assert!(!service.is_ready(&s.photos[0]), "the oldest went");
}

#[test]
fn walking_through_two_hundred_photos_with_the_next_ones_made_ahead_is_fast() {
    let s = setup(200);
    let service = s.engine.start_previews(2).unwrap();
    let mut times = Vec::new();
    for (i, id) in s.photos.iter().enumerate() {
        service.prefetch(&s.photos[(i + 1).min(s.photos.len())..(i + 3).min(s.photos.len())]);
        let start = Instant::now();
        assert!(ask(&service, *id).is_some());
        times.push(start.elapsed());
        // The person looks at the picture for a moment before the next key.
        std::thread::sleep(Duration::from_millis(15));
    }
    times.sort();
    let median = times[times.len() / 2];
    let worst = *times.last().unwrap();
    eprintln!("200 previews walked: median {median:?}, worst {worst:?}");
    assert!(
        median < Duration::from_millis(250),
        "a key press waited {median:?} for its picture at the median"
    );
}
