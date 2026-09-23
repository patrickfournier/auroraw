// SPDX-License-Identifier: GPL-3.0-or-later
//! Extracting the image a thumbnail is made from: the file itself for a standard format, the
//! largest embedded preview `rawler` can find for a RAW one (architecture §5.7). Never a full
//! RAW demosaic: that needs the image engine (M2), not built yet.

use std::path::Path;

use image::DynamicImage;

use crate::error::{ImagingError, Result};
use crate::format::is_standard;

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
