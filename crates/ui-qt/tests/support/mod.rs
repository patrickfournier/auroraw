// SPDX-License-Identifier: GPL-3.0-or-later
//! Machines for the QML suites: a folder standing for a person's computer (data, cache and Pictures
//! folders), empty or with a workspace of generated photos and a folder of more to add.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use auroraw_engine::{AddSourceRequest, Engine, Event, LocalDirs};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

/// Writes `count` small distinct JPEGs named `<prefix>_NNNN.jpg` in `folder`.
pub fn write_photos(folder: &Path, prefix: &str, count: u32) {
    std::fs::create_dir_all(folder).unwrap();
    for n in 0..count {
        let img = ImageBuffer::from_fn(160, 120, |x, y| {
            Rgb([
                (x as u8).wrapping_mul(2) ^ (n as u8).wrapping_mul(37),
                (y as u8).wrapping_mul(2),
                (n as u8).wrapping_mul(11),
            ])
        });
        DynamicImage::ImageRgb8(img)
            .save_with_format(
                folder.join(format!("{prefix}_{n:04}.jpg")),
                ImageFormat::Jpeg,
            )
            .unwrap();
    }
}

/// A machine that has one workspace, "Main", with a source of `photos` photos scanned into it.
pub fn machine_with_photos(home: &Path, photos: u32) {
    let dirs = LocalDirs {
        data: home.join("data"),
        cache: home.join("cache"),
    };
    let card = home.join("Card");
    write_photos(&card, "IMG", photos);
    let root: PathBuf = home.join("Pictures").join("Auroraw").join("Main");
    let opened = Engine::create_workspace(&root, "Main", &dirs).unwrap();
    let added = opened
        .engine
        .add_source(AddSourceRequest {
            root: card,
            name: Some("Card".into()),
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        match opened.events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished { job, .. }) if job == added.job => break,
            _ => assert!(Instant::now() < deadline, "the scan never finished"),
        }
    }
}
