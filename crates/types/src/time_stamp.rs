// SPDX-License-Identifier: GPL-3.0-or-later
use std::fmt;
use std::str::FromStr;

use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};

/// The text is not a UTC timestamp like `2026-09-21T14:02:11Z`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a UTC timestamp like 2026-09-21T14:02:11Z")]
pub struct TimestampParseError;

/// A UTC time with a precision of one second, written `2026-09-21T14:02:11Z` (note 002 §6.2).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(i64);

impl Timestamp {
    /// The current time.
    pub fn now() -> Self {
        Self(OffsetDateTime::now_utc().unix_timestamp())
    }

    /// From seconds since the Unix epoch.
    pub const fn from_unix(seconds: i64) -> Self {
        Self(seconds)
    }

    /// Seconds since the Unix epoch.
    pub const fn unix(&self) -> i64 {
        self.0
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `time` represents dates within about ±9999 years of the epoch, far more than this
        // application ever needs (photography did not exist before the 1820s), so this only
        // fails for a corrupted or adversarial value (spec: a hostile sidecar, testing strategy
        // §8). `Display` must not fail for that: returning `fmt::Error` here would make every
        // caller of the ubiquitous `.to_string()` panic ("a Display implementation returned an
        // error unexpectedly", since `fmt::Error` is meant for the formatter's own writer
        // failing, not for a value this crate cannot represent). Fall back to the raw seconds
        // instead: visibly not a valid `2026-09-21T14:02:11Z`-shaped string, so anyone reading it
        // (a person, `FromStr`) notices immediately, but nothing panics.
        match OffsetDateTime::from_unix_timestamp(self.0)
            .ok()
            .and_then(|t| t.format(&Rfc3339).ok())
        {
            Some(text) => f.write_str(&text),
            None => write!(f, "invalid-timestamp:{}", self.0),
        }
    }
}

impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Timestamp({self})")
    }
}

impl FromStr for Timestamp {
    type Err = TimestampParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Only UTC ("Z") with whole seconds: what is written is what is read.
        if !s.ends_with('Z') || s.contains('.') {
            return Err(TimestampParseError);
        }
        let t = OffsetDateTime::parse(s, &Rfc3339).map_err(|_| TimestampParseError)?;
        if t.offset() != UtcOffset::UTC {
            return Err(TimestampParseError);
        }
        Ok(Self(t.unix_timestamp()))
    }
}

impl serde::Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_utc_seconds() {
        let t = Timestamp::from_unix(1_790_000_531);
        let text = t.to_string();
        assert!(text.ends_with('Z') && text.len() == 20, "{text}");
        assert_eq!(text.parse::<Timestamp>(), Ok(t));
        assert_eq!(
            "2026-09-21T14:02:11Z"
                .parse::<Timestamp>()
                .unwrap()
                .to_string(),
            "2026-09-21T14:02:11Z"
        );
    }

    #[test]
    fn dates_before_the_epoch_round_trip() {
        // Real, if unusual, cases: a scanned negative from before 1970 (spec §5.6), or simply a
        // clock set wrong.
        for (secs, prefix) in [
            (-1, "1969-12-31T23:59:59"),
            (-86_400, "1969-12-31T00:00:00"),
            (-2_208_988_800, "1900-01-01T00:00:00"),
        ] {
            let t = Timestamp::from_unix(secs);
            let text = t.to_string();
            assert!(text.starts_with(prefix), "{secs} -> {text}");
            assert_eq!(text.parse::<Timestamp>(), Ok(t), "{secs}");
        }
    }

    #[test]
    fn a_value_time_cannot_represent_displays_visibly_instead_of_panicking() {
        // Far outside any real date, but a corrupted field could hold one (testing strategy §8):
        // `Display` (and so `.to_string()`, and serde's `collect_str`) must never panic.
        let t = Timestamp::from_unix(i64::MAX);
        let text = t.to_string();
        assert!(!text.ends_with('Z'), "{text}");
        assert!(
            text.parse::<Timestamp>().is_err(),
            "not silently accepted back either"
        );
    }

    #[test]
    fn refuses_other_forms() {
        for bad in [
            "",
            "2026-09-21",
            "2026-09-21T14:02:11",
            "2026-09-21T14:02:11+01:00",
            "2026-09-21T14:02:11.5Z",
        ] {
            assert!(bad.parse::<Timestamp>().is_err(), "{bad}");
        }
    }
}
