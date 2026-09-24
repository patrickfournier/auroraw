// SPDX-License-Identifier: GPL-3.0-or-later
//! `--make-fixture`: a workspace with a source of generated photos, under `SPIKE_HOME`, so that
//! the offscreen runs have something to show. Uses the engine only, no Qt.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use auroraw_engine::{AddSourceRequest, Engine, Event, LocalDirs};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

pub fn make() {
    let home = PathBuf::from(std::env::var_os("SPIKE_HOME").expect("SPIKE_HOME is set"));
    let photos: u32 = std::env::var("SPIKE_PHOTOS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(60);
    let dirs = LocalDirs {
        data: home.join("data"),
        cache: home.join("cache"),
    };
    let card = home.join("Card");
    std::fs::create_dir_all(&card).unwrap();
    for n in 0..photos {
        let img = ImageBuffer::from_fn(160, 120, |x, y| {
            Rgb([
                (x as u8).wrapping_mul(2) ^ (n as u8).wrapping_mul(37),
                (y as u8).wrapping_mul(2),
                (n as u8).wrapping_mul(11),
            ])
        });
        DynamicImage::ImageRgb8(img)
            .save_with_format(card.join(format!("IMG_{n:04}.jpg")), ImageFormat::Jpeg)
            .unwrap();
    }
    let root = home.join("Pictures").join("Auroraw").join("Main");
    let opened = Engine::create_workspace(&root, "Main", &dirs).unwrap();
    let added = opened
        .engine
        .add_source(AddSourceRequest {
            root: card,
            name: Some("Card".into()),
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match opened.events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished { job, .. }) if job == added.job => break,
            _ => assert!(Instant::now() < deadline, "the scan never finished"),
        }
    }
    println!("fixture: {photos} photos in {}", root.display());
}
