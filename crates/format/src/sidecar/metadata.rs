// SPDX-License-Identifier: GPL-3.0-or-later
//! The metadata shared by the photo sidecar and, as a derived copy, by the version sidecar
//! (design note 003 §4.3 and §7).

use auroraw_types::{ContentHash, KeywordId};

use super::extract as x;
use crate::xmp::{ArrayKind, Property, Xmp, ns};

/// The flag of a photo (D-063, spec §5.3). Stars and flag are kept apart (note 003 §4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// Picked.
    Picked,
    /// Rejected.
    Rejected,
}

impl Flag {
    fn as_text(self) -> &'static str {
        match self {
            Self::Picked => "picked",
            Self::Rejected => "rejected",
        }
    }
}

impl std::str::FromStr for Flag {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "picked" => Ok(Self::Picked),
            "rejected" => Ok(Self::Rejected),
            _ => Err(()),
        }
    }
}

/// A keyword of a photo as the sidecar records it: its identifier and the snapshot of its path
/// (note 003 §6). The names are a copy for other software; the identifier is the identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyword {
    /// The identifier in the vocabulary, absent for a keyword written by other software.
    pub id: Option<KeywordId>,
    /// The path with `|` between levels, as of the last write.
    pub path: String,
}

/// The original's capture data as read from the file, in the standard XMP properties (D-074).
/// Values are kept as XMP text; consumers convert them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Original {
    /// `exif:DateTimeOriginal`, ISO 8601 with the sub-seconds and the offset when known.
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
    /// `exif:PixelXDimension`.
    pub pixel_width: Option<u32>,
    /// `exif:PixelYDimension`.
    pub pixel_height: Option<u32>,
    /// `tiff:Orientation`, 1 to 8.
    pub orientation: Option<u32>,
    /// `exif:GPSLatitude`.
    pub gps_latitude: Option<String>,
    /// `exif:GPSLongitude`.
    pub gps_longitude: Option<String>,
    /// `exif:GPSAltitude`.
    pub gps_altitude: Option<String>,
}

/// The corrected position of an overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayGps {
    /// Latitude, in the XMP text form.
    pub latitude: String,
    /// Longitude, in the XMP text form.
    pub longitude: String,
    /// Altitude, if corrected.
    pub altitude: Option<String>,
    /// Fields this version does not know, kept.
    pub extra: Vec<Property>,
}

/// The EXIF corrections of the photographer, which leave the original's values visible (spec §5.7).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// A corrected capture time.
    pub capture_time: Option<String>,
    /// A corrected position.
    pub gps: Option<OverlayGps>,
    /// A camera name.
    pub camera: Option<String>,
    /// A lens name.
    pub lens: Option<String>,
    /// A corrected orientation.
    pub orientation: Option<u32>,
    /// Fields this version does not know, kept.
    pub extra: Vec<Property>,
}

/// The metadata of a photo, or the effective metadata copied into a version (note 003 §4.3, §7).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    /// Stars, 0 to 5.
    pub rating: Option<u8>,
    /// Picked or rejected.
    pub flag: Option<Flag>,
    /// The colour label's name.
    pub label: Option<String>,
    /// The title.
    pub title: Option<String>,
    /// The caption.
    pub caption: Option<String>,
    /// The identifiers of the keywords, aligned with `keyword_paths` when both have the same length.
    pub keyword_ids: Vec<KeywordId>,
    /// The paths of the keywords as of the last write (`dc:subject` holds their last levels).
    pub keyword_paths: Vec<String>,
    /// The creators.
    pub creator: Vec<String>,
    /// The copyright notice.
    pub rights: Option<String>,
    /// The usage terms.
    pub usage_terms: Option<String>,
    /// The web statement of rights.
    pub web_statement: Option<String>,
    /// The credit line.
    pub credit: Option<String>,
    /// The source.
    pub source: Option<String>,
    /// The headline.
    pub headline: Option<String>,
    /// The instructions.
    pub instructions: Option<String>,
    /// The sublocation.
    pub sublocation: Option<String>,
    /// The city.
    pub city: Option<String>,
    /// The region or state.
    pub region: Option<String>,
    /// The country.
    pub country: Option<String>,
    /// The ISO country code.
    pub country_code: Option<String>,
    /// The persons shown.
    pub persons: Vec<String>,
    /// The event.
    pub event: Option<String>,
    /// The original's capture data.
    pub original: Original,
    /// The EXIF overlay.
    pub overlay: Option<Overlay>,
}

fn leaf(path: &str) -> &str {
    path.rsplit('|').next().unwrap_or(path)
}

impl Metadata {
    /// Adds a keyword, with its identifier and the current path.
    pub fn push_keyword(&mut self, id: KeywordId, path: impl Into<String>) {
        self.keyword_ids.push(id);
        self.keyword_paths.push(path.into());
    }

    /// The keywords as pairs, when the identifiers and the paths line up.
    pub fn keywords(&self) -> Vec<Keyword> {
        if self.keyword_ids.len() == self.keyword_paths.len() {
            self.keyword_ids
                .iter()
                .zip(&self.keyword_paths)
                .map(|(id, path)| Keyword {
                    id: Some(*id),
                    path: path.clone(),
                })
                .collect()
        } else if self.keyword_ids.is_empty() {
            self.keyword_paths
                .iter()
                .map(|path| Keyword {
                    id: None,
                    path: path.clone(),
                })
                .collect()
        } else {
            // Identifiers and paths that do not line up: the identifiers are the truth.
            self.keyword_ids
                .iter()
                .map(|id| Keyword {
                    id: Some(*id),
                    path: String::new(),
                })
                .collect()
        }
    }

    /// The properties, in the canonical order of the schema.
    pub fn to_properties(&self) -> Vec<Property> {
        let o = &self.original;
        let leaves: Vec<String> = self
            .keyword_paths
            .iter()
            .map(|p| leaf(p).to_string())
            .collect();
        let ids: Vec<String> = self.keyword_ids.iter().map(ToString::to_string).collect();
        let mut props: Vec<Option<Property>> = vec![
            self.rating
                .map(|r| Property::text(ns::XMP, "Rating", r.to_string())),
            self.flag
                .map(|f| Property::text(ns::AUR, "Flag", f.as_text())),
            x::opt_text(ns::XMP, "Label", &self.label),
            x::opt_lang(ns::DC, "title", &self.title),
            x::opt_lang(ns::DC, "description", &self.caption),
            x::opt_array(ns::DC, "subject", ArrayKind::Bag, &leaves),
            x::opt_array(
                ns::LR,
                "hierarchicalSubject",
                ArrayKind::Bag,
                &self.keyword_paths,
            ),
            x::opt_array(ns::AUR, "KeywordIds", ArrayKind::Bag, &ids),
            x::opt_array(ns::DC, "creator", ArrayKind::Seq, &self.creator),
            x::opt_lang(ns::DC, "rights", &self.rights),
            x::opt_lang(ns::XMP_RIGHTS, "UsageTerms", &self.usage_terms),
            x::opt_text(ns::XMP_RIGHTS, "WebStatement", &self.web_statement),
            x::opt_text(ns::PHOTOSHOP, "Credit", &self.credit),
            x::opt_text(ns::PHOTOSHOP, "Source", &self.source),
            x::opt_text(ns::PHOTOSHOP, "Headline", &self.headline),
            x::opt_text(ns::PHOTOSHOP, "Instructions", &self.instructions),
            x::opt_text(ns::IPTC_CORE, "Location", &self.sublocation),
            x::opt_text(ns::PHOTOSHOP, "City", &self.city),
            x::opt_text(ns::PHOTOSHOP, "State", &self.region),
            x::opt_text(ns::PHOTOSHOP, "Country", &self.country),
            x::opt_text(ns::IPTC_CORE, "CountryCode", &self.country_code),
            x::opt_array(ns::IPTC_EXT, "PersonInImage", ArrayKind::Bag, &self.persons),
            x::opt_lang(ns::IPTC_EXT, "Event", &self.event),
            x::opt_text(ns::EXIF, "DateTimeOriginal", &o.capture_time),
            x::opt_text(ns::TIFF, "Make", &o.make),
            x::opt_text(ns::TIFF, "Model", &o.model),
            x::opt_text(ns::AUX, "SerialNumber", &o.serial),
            x::opt_text(ns::AUX, "Lens", &o.lens),
            x::opt_text(ns::EXIF, "ExposureTime", &o.exposure_time),
            x::opt_text(ns::EXIF, "FNumber", &o.f_number),
            x::opt_array(ns::EXIF, "ISOSpeedRatings", ArrayKind::Seq, &o.iso),
            x::opt_text(ns::EXIF, "FocalLength", &o.focal_length),
            x::opt_text(ns::EXIF, "FocalLengthIn35mmFilm", &o.focal_length_35mm),
            o.pixel_width
                .map(|v| Property::text(ns::EXIF, "PixelXDimension", v.to_string())),
            o.pixel_height
                .map(|v| Property::text(ns::EXIF, "PixelYDimension", v.to_string())),
            o.orientation
                .map(|v| Property::text(ns::TIFF, "Orientation", v.to_string())),
            x::opt_text(ns::EXIF, "GPSLatitude", &o.gps_latitude),
            x::opt_text(ns::EXIF, "GPSLongitude", &o.gps_longitude),
            x::opt_text(ns::EXIF, "GPSAltitude", &o.gps_altitude),
        ];
        props.push(self.overlay.as_ref().map(overlay_property));
        props.into_iter().flatten().collect()
    }

    /// Takes the properties this version understands out of `props`; the rest stay.
    pub fn take_from(props: &mut Vec<Property>) -> Self {
        let mut m = Self {
            rating: x::parsed::<u8>(props, ns::XMP, "Rating").filter(|r| *r <= 5),
            flag: x::parsed(props, ns::AUR, "Flag"),
            label: x::text(props, ns::XMP, "Label"),
            title: x::lang_text(props, ns::DC, "title"),
            caption: x::lang_text(props, ns::DC, "description"),
            ..Self::default()
        };
        let flat = x::texts(props, ns::DC, "subject");
        let paths = x::texts(props, ns::LR, "hierarchicalSubject");
        m.keyword_ids = x::parsed_array(props, ns::AUR, "KeywordIds").unwrap_or_default();
        m.keyword_paths = paths.or(flat).unwrap_or_default();
        m.creator = x::texts(props, ns::DC, "creator").unwrap_or_default();
        m.rights = x::lang_text(props, ns::DC, "rights");
        m.usage_terms = x::lang_text(props, ns::XMP_RIGHTS, "UsageTerms");
        m.web_statement = x::text(props, ns::XMP_RIGHTS, "WebStatement");
        m.credit = x::text(props, ns::PHOTOSHOP, "Credit");
        m.source = x::text(props, ns::PHOTOSHOP, "Source");
        m.headline = x::text(props, ns::PHOTOSHOP, "Headline");
        m.instructions = x::text(props, ns::PHOTOSHOP, "Instructions");
        m.sublocation = x::text(props, ns::IPTC_CORE, "Location");
        m.city = x::text(props, ns::PHOTOSHOP, "City");
        m.region = x::text(props, ns::PHOTOSHOP, "State");
        m.country = x::text(props, ns::PHOTOSHOP, "Country");
        m.country_code = x::text(props, ns::IPTC_CORE, "CountryCode");
        m.persons = x::texts(props, ns::IPTC_EXT, "PersonInImage").unwrap_or_default();
        m.event = x::lang_text(props, ns::IPTC_EXT, "Event");
        let o = &mut m.original;
        o.capture_time = x::text(props, ns::EXIF, "DateTimeOriginal");
        o.make = x::text(props, ns::TIFF, "Make");
        o.model = x::text(props, ns::TIFF, "Model");
        o.serial = x::text(props, ns::AUX, "SerialNumber");
        o.lens = x::text(props, ns::AUX, "Lens");
        o.exposure_time = x::text(props, ns::EXIF, "ExposureTime");
        o.f_number = x::text(props, ns::EXIF, "FNumber");
        o.iso = x::texts(props, ns::EXIF, "ISOSpeedRatings").unwrap_or_default();
        o.focal_length = x::text(props, ns::EXIF, "FocalLength");
        o.focal_length_35mm = x::text(props, ns::EXIF, "FocalLengthIn35mmFilm");
        o.pixel_width = x::parsed(props, ns::EXIF, "PixelXDimension");
        o.pixel_height = x::parsed(props, ns::EXIF, "PixelYDimension");
        o.orientation = x::parsed(props, ns::TIFF, "Orientation");
        o.gps_latitude = x::text(props, ns::EXIF, "GPSLatitude");
        o.gps_longitude = x::text(props, ns::EXIF, "GPSLongitude");
        o.gps_altitude = x::text(props, ns::EXIF, "GPSAltitude");
        m.overlay = x::structure(props, ns::AUR, "Overlay").map(overlay_from_fields);
        m
    }

    /// The digest of the fields that are copied into version sidecars (note 003 §5.3): BLAKE3 of
    /// their canonical form, with the keywords counted by identifier, not by name (§6).
    pub fn digest(&self) -> ContentHash {
        let mut props = self.to_properties();
        if !self.keyword_ids.is_empty() {
            props.retain(|p| !p.is(ns::DC, "subject") && !p.is(ns::LR, "hierarchicalSubject"));
        }
        let bytes = Xmp {
            properties: props,
            prefixes: Vec::new(),
        }
        .to_bytes();
        ContentHash::from_bytes(*blake3::hash(&bytes).as_bytes())
    }
}

fn overlay_property(o: &Overlay) -> Property {
    let mut fields = Vec::new();
    fields.extend(x::opt_text(ns::AUR, "CaptureTime", &o.capture_time));
    if let Some(g) = &o.gps {
        let mut gps = vec![
            Property::text(ns::AUR, "Latitude", g.latitude.clone()),
            Property::text(ns::AUR, "Longitude", g.longitude.clone()),
        ];
        gps.extend(x::opt_text(ns::AUR, "Altitude", &g.altitude));
        gps.extend(g.extra.iter().cloned());
        fields.push(Property::structure(ns::AUR, "Gps", gps));
    }
    fields.extend(x::opt_text(ns::AUR, "Camera", &o.camera));
    fields.extend(x::opt_text(ns::AUR, "Lens", &o.lens));
    fields.extend(
        o.orientation
            .map(|v| Property::text(ns::AUR, "Orientation", v.to_string())),
    );
    fields.extend(o.extra.iter().cloned());
    Property::structure(ns::AUR, "Overlay", fields)
}

fn overlay_from_fields(mut fields: Vec<Property>) -> Overlay {
    let mut gps = None;
    if let Some(mut g) = x::structure(&mut fields, ns::AUR, "Gps") {
        let original = g.clone();
        match (
            x::text(&mut g, ns::AUR, "Latitude"),
            x::text(&mut g, ns::AUR, "Longitude"),
        ) {
            (Some(latitude), Some(longitude)) => {
                let altitude = x::text(&mut g, ns::AUR, "Altitude");
                gps = Some(OverlayGps {
                    latitude,
                    longitude,
                    altitude,
                    extra: g,
                });
            }
            // Not understood: keep the structure as it was.
            _ => fields.push(Property::structure(ns::AUR, "Gps", original)),
        }
    }
    let capture_time = x::text(&mut fields, ns::AUR, "CaptureTime");
    let camera = x::text(&mut fields, ns::AUR, "Camera");
    let lens = x::text(&mut fields, ns::AUR, "Lens");
    let orientation = x::parsed(&mut fields, ns::AUR, "Orientation");
    Overlay {
        capture_time,
        gps,
        camera,
        lens,
        orientation,
        extra: fields,
    }
}
