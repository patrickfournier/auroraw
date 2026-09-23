// SPDX-License-Identifier: GPL-3.0-or-later
//! GPX matching (spec §5.2): a photo's position, found in a recorded track by its capture time,
//! corrected for the camera's clock offset and time zone (M1 plan §6, item 6). A camera's clock
//! reads local time with no time zone of its own; [`corrected_time`] is what turns that reading
//! into the true UTC instant a GPX track's own timestamps (always UTC, per the format) can be
//! looked up against.

use time::{Duration, OffsetDateTime, PrimitiveDateTime, UtcOffset};

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::{ImportError, Result};

/// One point a GPS track recorded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trackpoint {
    /// When it was recorded, in UTC (GPX's own convention).
    pub time: OffsetDateTime,
    /// Latitude, in degrees.
    pub lat: f64,
    /// Longitude, in degrees.
    pub lon: f64,
    /// Elevation in metres, if the track recorded one.
    pub elevation: Option<f64>,
}

/// A photo's position, interpolated from a track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsFix {
    /// Latitude, in degrees.
    pub lat: f64,
    /// Longitude, in degrees.
    pub lon: f64,
    /// Elevation in metres, when both bracketing points had one.
    pub elevation: Option<f64>,
}

fn attr(e: &quick_xml::events::BytesStart, name: &str) -> Option<f64> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| a.value.parse().ok())
}

/// Parses every `trkpt` in `xml`, across every track and segment (a GPX file can have several of
/// each), sorted by time. A point with no `<time>` or an unparseable one is skipped: elevation and
/// waypoints without a time are common in real files and are simply not usable for matching.
pub fn parse(xml: &[u8]) -> Result<Vec<Trackpoint>> {
    let text = std::str::from_utf8(xml).map_err(xml_error)?;
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut points = Vec::new();
    let (mut lat, mut lon) = (None, None);
    let (mut in_time, mut in_ele) = (false, false);
    let (mut time, mut ele) = (None, None);
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == "trkpt" => {
                lat = attr(&e, "lat");
                lon = attr(&e, "lon");
                time = None;
                ele = None;
            }
            Event::Start(e) if e.name().as_ref() == "time" => in_time = true,
            Event::End(e) if e.name().as_ref() == "time" => in_time = false,
            Event::Start(e) if e.name().as_ref() == "ele" => in_ele = true,
            Event::End(e) if e.name().as_ref() == "ele" => in_ele = false,
            Event::Text(t) if in_time => {
                time = OffsetDateTime::parse(
                    t.as_ref(),
                    &time::format_description::well_known::Rfc3339,
                )
                .ok();
            }
            Event::Text(t) if in_ele => {
                ele = t.as_ref().parse().ok();
            }
            Event::End(e) if e.name().as_ref() == "trkpt" => {
                if let (Some(lat), Some(lon), Some(time)) = (lat, lon, time) {
                    points.push(Trackpoint {
                        time,
                        lat,
                        lon,
                        elevation: ele,
                    });
                }
            }
            _ => {}
        }
    }
    points.sort_by_key(|p| p.time);
    Ok(points)
}

fn xml_error(e: impl std::fmt::Display) -> ImportError {
    ImportError::Io {
        path: std::path::PathBuf::from("<gpx>"),
        source: std::io::Error::other(e.to_string()),
    }
}

/// The camera's own clock reading (`local`, no time zone), interpreted as being in
/// `camera_timezone` and corrected by `clock_offset` (added; positive when the camera's clock was
/// ahead of true time), giving the true UTC instant.
pub fn corrected_time(
    local: PrimitiveDateTime,
    camera_timezone: UtcOffset,
    clock_offset: Duration,
) -> OffsetDateTime {
    local.assume_offset(camera_timezone) - clock_offset
}

/// The photo's position at `at`, linearly interpolated between the two points of `track`
/// bracketing it (sorted by [`parse`]). `None` if `track` is empty or `at` falls outside its
/// span: matching does not extrapolate a position the track never recorded.
pub fn position_at(track: &[Trackpoint], at: OffsetDateTime) -> Option<GpsFix> {
    if track.is_empty() || at < track[0].time || at > track[track.len() - 1].time {
        return None;
    }
    let i = track.partition_point(|p| p.time <= at);
    let after = track.get(i)?;
    if after.time == at {
        return Some(GpsFix {
            lat: after.lat,
            lon: after.lon,
            elevation: after.elevation,
        });
    }
    let before = &track[i - 1];
    let span = (after.time - before.time).as_seconds_f64();
    let t = if span > 0.0 {
        (at - before.time).as_seconds_f64() / span
    } else {
        0.0
    };
    Some(GpsFix {
        lat: before.lat + (after.lat - before.lat) * t,
        lon: before.lon + (after.lon - before.lon) * t,
        elevation: match (before.elevation, after.elevation) {
            (Some(a), Some(b)) => Some(a + (b - a) * t),
            _ => None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{datetime, offset};

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<gpx>
  <trk><trkseg>
    <trkpt lat="45.5" lon="-73.5"><ele>10</ele><time>2026-09-23T14:00:00Z</time></trkpt>
    <trkpt lat="45.6" lon="-73.6"><ele>20</ele><time>2026-09-23T14:10:00Z</time></trkpt>
  </trkseg></trk>
</gpx>"#;

    #[test]
    fn parses_points_in_order() {
        let points = parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].lat, 45.5);
        assert_eq!(points[1].time, datetime!(2026-09-23 14:10:00 UTC));
    }

    #[test]
    fn interpolates_halfway_between_two_points() {
        let points = parse(SAMPLE.as_bytes()).unwrap();
        let fix = position_at(&points, datetime!(2026-09-23 14:05:00 UTC)).unwrap();
        assert!((fix.lat - 45.55).abs() < 1e-9);
        assert!((fix.lon - (-73.55)).abs() < 1e-9);
        assert_eq!(fix.elevation, Some(15.0));
    }

    #[test]
    fn a_time_outside_the_track_has_no_fix() {
        let points = parse(SAMPLE.as_bytes()).unwrap();
        assert!(position_at(&points, datetime!(2026-09-23 13:00:00 UTC)).is_none());
        assert!(position_at(&points, datetime!(2026-09-23 15:00:00 UTC)).is_none());
    }

    #[test]
    fn an_exact_match_returns_that_points_own_position() {
        let points = parse(SAMPLE.as_bytes()).unwrap();
        let fix = position_at(&points, datetime!(2026-09-23 14:00:00 UTC)).unwrap();
        assert_eq!(fix.lat, 45.5);
        assert_eq!(fix.elevation, Some(10.0));
    }

    #[test]
    fn corrected_time_applies_the_timezone_then_the_clock_offset() {
        // The camera's clock reads local time in UTC-4, and is running 3 minutes fast.
        let local = datetime!(2026-09-23 10:03:00);
        let corrected = corrected_time(local, offset!(-4), Duration::minutes(3));
        assert_eq!(corrected, datetime!(2026-09-23 14:00:00 UTC));
    }

    #[test]
    fn an_empty_track_has_no_fix() {
        assert!(position_at(&[], datetime!(2026-01-01 0:00 UTC)).is_none());
    }
}
