// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading a photo's metadata: the cache of the original's data a photo sidecar keeps (design
//! note 003 §4.3, D-074), read once so a rebuild never needs to reopen the file (design note 004
//! §8, spike 3: 20 ms cold per file, impossible for an offline source).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use rawler::formats::tiff::value::Rational;

use crate::error::{ImagingError, Result};
use crate::format::is_standard;

/// What the sidecar's `Original` caches, read from the file itself. Field shapes mirror
/// `auroraw_format::sidecar::Original` deliberately: this crate does not depend on `format`
/// (architecture §3.2, the two are siblings under `engine`), so translating one into the other,
/// when WP7 wires the two together, is a straight field-by-field copy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    /// `exif:DateTimeOriginal`, ISO 8601 text, as the file wrote it.
    pub capture_time: Option<String>,
    /// `tiff:Make`.
    pub make: Option<String>,
    /// `tiff:Model`.
    pub model: Option<String>,
    /// `aux:SerialNumber`.
    pub serial: Option<String>,
    /// `aux:Lens`.
    pub lens: Option<String>,
    /// `exif:ExposureTime`, a rational such as `1/250`.
    pub exposure_time: Option<String>,
    /// `exif:FNumber`, a rational such as `28/10`.
    pub f_number: Option<String>,
    /// `exif:ISOSpeedRatings`.
    pub iso: Vec<String>,
    /// `exif:FocalLength`.
    pub focal_length: Option<String>,
    /// `exif:FocalLengthIn35mmFilm`.
    pub focal_length_35mm: Option<String>,
    /// `exif:PixelXDimension`: the embedded preview's width, not the sensor's (this crate makes
    /// no full RAW decode without the image engine, M2).
    pub pixel_width: Option<u32>,
    /// `exif:PixelYDimension`, likewise the preview's height.
    pub pixel_height: Option<u32>,
    /// `tiff:Orientation`, 1 to 8.
    pub orientation: Option<u32>,
    /// `exif:GPSLatitude`, XMP `GPSCoordinate` text (`"DDD,MM.mmmmmmR"`).
    pub gps_latitude: Option<String>,
    /// `exif:GPSLongitude`, likewise.
    pub gps_longitude: Option<String>,
    /// `exif:GPSAltitude`, a rational in metres.
    pub gps_altitude: Option<String>,
}

fn rational_text(r: &Rational) -> String {
    format!("{}/{}", r.n, r.d)
}

fn xmp_coordinate(whole: &[Rational; 3], reference: &str) -> String {
    let degrees = whole[0].n as f64 / whole[0].d.max(1) as f64;
    let minutes = whole[1].n as f64 / whole[1].d.max(1) as f64;
    let seconds = whole[2].n as f64 / whole[2].d.max(1) as f64;
    format!(
        "{},{:.6}{}",
        degrees as i64,
        minutes + seconds / 60.0,
        reference
    )
}

/// Reads what this crate can find, dispatching by extension ([`is_standard`]). Never a full RAW
/// decode: only the container's own metadata and, for the pixel dimensions, the embedded
/// preview's (WP5 does not build the full demosaic path; that needs the image engine, M2).
pub fn read_metadata(path: &Path) -> Result<Metadata> {
    if is_standard(path) {
        read_standard_metadata(path)
    } else {
        read_raw_metadata(path)
    }
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

fn read_raw_metadata(path: &Path) -> Result<Metadata> {
    let source = rawler::rawsource::RawSource::new(path).map_err(|e| io_err(path, e))?;
    let decoder = rawler::get_decoder(&source).map_err(|e| decode_err(path, e))?;
    let params = rawler::decoders::RawDecodeParams::default();
    let raw_meta = decoder
        .raw_metadata(&source, &params)
        .map_err(|e| decode_err(path, e))?;
    let exif = &raw_meta.exif;

    let iso = exif
        .iso_speed
        .map(|v| v.to_string())
        .or_else(|| exif.iso_speed_ratings.map(|v| v.to_string()))
        .into_iter()
        .collect();

    let (gps_latitude, gps_longitude, gps_altitude) = match &exif.gps {
        Some(gps) => (
            match (&gps.gps_latitude, &gps.gps_latitude_ref) {
                (Some(v), Some(r)) => Some(xmp_coordinate(v, r)),
                _ => None,
            },
            match (&gps.gps_longitude, &gps.gps_longitude_ref) {
                (Some(v), Some(r)) => Some(xmp_coordinate(v, r)),
                _ => None,
            },
            gps.gps_altitude.as_ref().map(rational_text),
        ),
        None => (None, None, None),
    };

    Ok(Metadata {
        capture_time: exif.date_time_original.clone(),
        make: (!raw_meta.make.is_empty()).then_some(raw_meta.make.clone()),
        model: (!raw_meta.model.is_empty()).then_some(raw_meta.model.clone()),
        serial: exif.serial_number.clone(),
        lens: exif.lens_model.clone(),
        exposure_time: exif.exposure_time.as_ref().map(rational_text),
        f_number: exif.fnumber.as_ref().map(rational_text),
        iso,
        focal_length: exif.focal_length.as_ref().map(rational_text),
        focal_length_35mm: None, // rawler's Exif has no dedicated field for this (a real gap).
        pixel_width: None,       // filled from the embedded preview by `crate::preview`, if any.
        pixel_height: None,
        orientation: exif.orientation.map(u32::from),
        gps_latitude,
        gps_longitude,
        gps_altitude,
    })
}

fn read_standard_metadata(path: &Path) -> Result<Metadata> {
    let (pixel_width, pixel_height) = image::ImageReader::open(path)
        .map_err(|e| io_err(path, e))?
        .with_guessed_format()
        .map_err(|e| io_err(path, e))?
        .into_dimensions()
        .map(|(w, h)| (Some(w), Some(h)))
        .unwrap_or((None, None));

    let mut metadata = Metadata {
        pixel_width,
        pixel_height,
        ..Metadata::default()
    };

    let file = File::open(path).map_err(|e| io_err(path, e))?;
    let Ok(exif) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) else {
        // No EXIF segment at all (common for PNG): the dimensions above are still useful.
        return Ok(metadata);
    };

    let text = |tag: exif::Tag| {
        exif.get_field(tag, exif::In::PRIMARY)
            .map(|f| f.display_value().to_string())
    };
    let rational = |tag: exif::Tag| match exif.get_field(tag, exif::In::PRIMARY).map(|f| &f.value) {
        Some(exif::Value::Rational(v)) if !v.is_empty() => {
            Some(format!("{}/{}", v[0].num, v[0].denom))
        }
        _ => None,
    };

    metadata.capture_time = text(exif::Tag::DateTimeOriginal);
    metadata.make = text(exif::Tag::Make);
    metadata.model = text(exif::Tag::Model);
    metadata.serial = text(exif::Tag::BodySerialNumber);
    metadata.lens = text(exif::Tag::LensModel);
    metadata.exposure_time = rational(exif::Tag::ExposureTime);
    metadata.f_number = rational(exif::Tag::FNumber);
    metadata.iso = match exif
        .get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY)
        .map(|f| &f.value)
    {
        Some(exif::Value::Short(v)) => v.iter().map(|n| n.to_string()).collect(),
        _ => Vec::new(),
    };
    metadata.focal_length = rational(exif::Tag::FocalLength);
    metadata.focal_length_35mm = text(exif::Tag::FocalLengthIn35mmFilm);
    metadata.orientation = match exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .map(|f| &f.value)
    {
        Some(exif::Value::Short(v)) => v.first().map(|&n| n as u32),
        _ => None,
    };

    let gps_rational =
        |tag: exif::Tag| match exif.get_field(tag, exif::In::PRIMARY).map(|f| &f.value) {
            Some(exif::Value::Rational(v)) if v.len() == 3 => Some([
                Rational {
                    n: v[0].num,
                    d: v[0].denom,
                },
                Rational {
                    n: v[1].num,
                    d: v[1].denom,
                },
                Rational {
                    n: v[2].num,
                    d: v[2].denom,
                },
            ]),
            _ => None,
        };
    if let (Some(coord), Some(reference)) = (
        gps_rational(exif::Tag::GPSLatitude),
        text(exif::Tag::GPSLatitudeRef),
    ) {
        metadata.gps_latitude = Some(xmp_coordinate(&coord, &reference));
    }
    if let (Some(coord), Some(reference)) = (
        gps_rational(exif::Tag::GPSLongitude),
        text(exif::Tag::GPSLongitudeRef),
    ) {
        metadata.gps_longitude = Some(xmp_coordinate(&coord, &reference));
    }
    metadata.gps_altitude = rational(exif::Tag::GPSAltitude);

    Ok(metadata)
}
