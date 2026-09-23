// SPDX-License-Identifier: GPL-3.0-or-later
//! Thumbnail generation throughput (spike 3: 10.4 ms per thumbnail on one core, decode + resize +
//! encode, measured **from a 1.6 MP preview already in hand** -- not the decode of a whole RAW
//! file). A measurement, not a gate (testing strategy §7): run on demand, in release mode for a
//! number that means anything --
//!
//! `cargo test --release -p auroraw-imaging --test throughput -- --ignored --nocapture`

use std::path::{Path, PathBuf};
use std::time::Instant;

const SAMPLES: &[&str] = &[
    "B13A0732.CR2",
    "Nikon-D850-14bit-compressed.NEF",
    "DSC00396.ARW",
    "DSCF0120.RAF",
    "dc-s5_6k4k.RW2",
];

/// The long edge spike 3's own fixed-size preview had. Real embedded previews vary hugely by
/// camera (this crate's five RAW samples ranged from 1.7 MP to 45 MP when this was measured,
/// nowhere near a fair comparison against spike 3's own number on their own) -- normalising to
/// the same input spike 3 used is what makes the two numbers comparable at all. Decoding a real,
/// possibly much larger embedded preview is a separate, real cost of its own (see this file's own
/// status note in the M1 plan): not what this measurement is isolating.
const SPIKE3_PREVIEW_LONG_EDGE: u32 = 1600;

#[test]
#[ignore = "a measurement, run on demand (see this file's own doc comment)"]
fn thumbnail_generation_rate() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/samples");
    let previews: Vec<_> = SAMPLES
        .iter()
        .filter_map(|name| auroraw_imaging::embedded_preview(&dir.join(name)).ok())
        .map(|preview| {
            // Normalise to spike 3's own input size, outside the timed loop: see
            // `SPIKE3_PREVIEW_LONG_EDGE`.
            let thumb = auroraw_imaging::make_thumbnail(
                preview,
                None,
                SPIKE3_PREVIEW_LONG_EDGE,
                Path::new("normalising"),
            )
            .unwrap();
            image::load_from_memory(&thumb.jpeg).unwrap()
        })
        .collect();
    assert!(!previews.is_empty(), "run tools/fetch-samples.sh first");
    for (name, preview) in SAMPLES.iter().zip(&previews) {
        eprintln!(
            "{name}: normalised to {}x{}",
            preview.width(),
            preview.height()
        );
    }

    let iterations = 1000;
    let start = Instant::now();
    for i in 0..iterations {
        let preview = previews[i % previews.len()].clone();
        auroraw_imaging::make_thumbnail(preview, None, 256, Path::new("throughput-measurement"))
            .unwrap();
    }
    let elapsed = start.elapsed();
    let per_second = iterations as f64 / elapsed.as_secs_f64();
    println!(
        "{iterations} thumbnails (resize + encode from a {SPIKE3_PREVIEW_LONG_EDGE} px preview) in {elapsed:?}: {per_second:.0}/s, one thread"
    );

    // A generous floor, not spike 3's own ~96/s single-core figure: this runs in whatever
    // profile the caller chose (debug by default, tens of times slower), on whatever machine CI
    // happens to be, and is here to catch a real regression (an accidental full re-decode, a
    // resize that stopped short-circuiting same-size images), not to gate on absolute speed.
    assert!(
        per_second > 5.0,
        "{per_second:.0}/s looks like a regression, not noise"
    );
}
