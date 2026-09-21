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
        let t = OffsetDateTime::from_unix_timestamp(self.0).map_err(|_| fmt::Error)?;
        f.write_str(&t.format(&Rfc3339).map_err(|_| fmt::Error)?)
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
