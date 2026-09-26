// SPDX-License-Identifier: GPL-3.0-or-later
//! Extracting the image a thumbnail is made from: the file itself for a standard format, the
//! largest embedded preview `rawler` can find for a RAW one (architecture §5.7). Never a full
//! RAW demosaic: that needs the image engine (M2), not built yet.

use std::path::Path;

use image::DynamicImage;

use crate::error::{ImagingError, Result};
use crate::format::is_standard;
use crate::thumbnail::{Thumbnail, make_scaled};

/// The best available preview for `path`. `Err(`[`ImagingError::NoPreview`]`)` if a RAW file
/// carries none: rare (every sample this crate is tested against has one), and correct to
/// refuse rather than guess.
pub fn embedded_preview(path: &Path) -> Result<DynamicImage> {
    if is_standard(path) {
        image::ImageReader::open(path)
            .map_err(|e| io_err(path, e))?
            .with_guessed_format()
            .map_err(|e| io_err(path, e))?
            .decode()
            .map_err(|e| decode_err(path, e))
    } else {
        raw_preview(path)
    }
}

/// The long edge, in pixels, the image view is served at most: an embedded preview (1.6 to 8 MP) goes whole, a
/// standard 24 MP file is reduced, so that a decoded image never costs much more than 45 MB.
pub const VIEW_MAX_EDGE: u32 = 4096;

/// What the image view shows for `path`: [`embedded_preview`], upright (`orientation` is the sidecar's
/// `tiff:Orientation`), at most `max_edge` pixels on its long side, as a JPEG at quality 90 (a picture to
/// look at, not a thumbnail).
pub fn view_image(path: &Path, orientation: Option<u32>, max_edge: u32) -> Result<Thumbnail> {
    make_scaled(embedded_preview(path)?, orientation, max_edge, 90, path)
}

fn raw_preview(path: &Path) -> Result<DynamicImage> {
    let source = rawler::rawsource::RawSource::new(path).map_err(|e| io_err(path, e))?;
    let decoder = rawler::get_decoder(&source).map_err(|e| decode_err(path, e))?;
    let params = rawler::decoders::RawDecodeParams::default();
    // The larger embedded preview first (what spike 3 measured from, about 1.6 MP): the small
    // camera-LCD thumbnail is the fallback, not the default.
    if let Some(image) = decoder
        .preview_image(&source, &params)
        .map_err(|e| decode_err(path, e))?
    {
        return Ok(image);
    }
    if let Some(image) = decoder
        .thumbnail_image(&source, &params)
        .map_err(|e| decode_err(path, e))?
    {
        return Ok(image);
    }
    Err(ImagingError::NoPreview {
        path: path.to_path_buf(),
    })
}

fn io_err(path: &Path, source: std::io::Error) -> ImagingError {
    ImagingError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn decode_err(path: &Path, message: impl std::fmt::Display) -> ImagingError {
    ImagingError::Decode {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

    fn jpeg(path: &Path, width: u32, height: u32) {
        let img = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x % 251) as u8, (y % 241) as u8, 90])
        });
        DynamicImage::ImageRgb8(img)
            .save_with_format(path, ImageFormat::Jpeg)
            .unwrap();
    }

    fn size_of(image: &Thumbnail) -> (u32, u32) {
        let decoded = image::load_from_memory(&image.jpeg).unwrap();
        (decoded.width(), decoded.height())
    }

    #[test]
    fn the_view_image_is_upright_and_never_larger_than_the_cap() {
        let dir = auroraw_testkit::temp_dir();
        let file = dir.path().join("wide.jpg");
        jpeg(&file, 600, 400);

        let whole = view_image(&file, None, VIEW_MAX_EDGE).unwrap();
        assert_eq!(
            (whole.width, whole.height),
            (600, 400),
            "not enlarged, not reduced"
        );
        assert_eq!(size_of(&whole), (600, 400));

        let turned = view_image(&file, Some(6), VIEW_MAX_EDGE).unwrap();
        assert_eq!(
            (turned.width, turned.height),
            (400, 600),
            "orientation 6 is a quarter turn"
        );

        let capped = view_image(&file, None, 300).unwrap();
        assert_eq!(
            (capped.width, capped.height),
            (300, 200),
            "the long edge is the cap"
        );
        assert_eq!(size_of(&capped), (300, 200));
    }

    #[test]
    fn a_file_that_is_not_an_image_is_an_error_not_a_panic() {
        let dir = auroraw_testkit::temp_dir();
        let file = dir.path().join("broken.jpg");
        std::fs::write(&file, b"not a picture").unwrap();
        assert!(view_image(&file, None, VIEW_MAX_EDGE).is_err());
        assert!(view_image(&dir.path().join("missing.jpg"), None, VIEW_MAX_EDGE).is_err());
    }
}
