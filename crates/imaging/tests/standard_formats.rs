// SPDX-License-Identifier: GPL-3.0-or-later
//! JPEG, PNG and TIFF, decoded directly (no embedded preview to extract: the file already is
//! the image). Synthetic fixtures, generated in the test, so nothing external is needed.

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

fn write_test_image(path: &std::path::Path, format: ImageFormat) {
    let img = ImageBuffer::from_fn(64, 48, |x, y| Rgb([(x * 4) as u8, (y * 5) as u8, 128]));
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, format)
        .unwrap();
}

#[test]
fn jpeg_png_and_tiff_all_decode_to_a_thumbnail_never_upscaled() {
    let dir = auroraw_testkit::temp_dir();
    for (name, format) in [
        ("a.jpg", ImageFormat::Jpeg),
        ("a.png", ImageFormat::Png),
        ("a.tif", ImageFormat::Tiff),
    ] {
        let path = dir.path().join(name);
        write_test_image(&path, format);

        let (metadata, thumbnail, _hash) =
            auroraw_imaging::process(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(metadata.pixel_width, Some(64), "{name}");
        assert_eq!(metadata.pixel_height, Some(48), "{name}");
        assert!(!thumbnail.jpeg.is_empty(), "{name}");
        // Smaller than the 256 px target already: never upscaled.
        assert_eq!((thumbnail.width, thumbnail.height), (64, 48), "{name}");
    }
}

#[test]
fn a_corrupt_standard_file_fails_cleanly_not_a_panic() {
    let dir = auroraw_testkit::temp_dir();
    let path = dir.path().join("bad.jpg");
    std::fs::write(&path, b"not a jpeg at all, just some arbitrary bytes").unwrap();
    assert!(auroraw_imaging::embedded_preview(&path).is_err());
}

#[test]
fn an_unrecognised_extension_is_refused_cleanly() {
    let dir = auroraw_testkit::temp_dir();
    let path = dir.path().join("photo.xyz");
    std::fs::write(&path, b"whatever").unwrap();
    assert!(auroraw_imaging::read_metadata(&path).is_err());
    assert!(auroraw_imaging::embedded_preview(&path).is_err());
}
