// SPDX-License-Identifier: GPL-3.0-or-later
use std::fmt;
use std::str::FromStr;

/// The version of a file format or of the catalogue schema, an integer written inside the files
/// (architecture §5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion(pub u32);

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The text is not a schema version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSchemaVersionError;

impl fmt::Display for ParseSchemaVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a schema version: expected a non-negative integer")
    }
}

impl std::error::Error for ParseSchemaVersionError {}

impl FromStr for SchemaVersion {
    type Err = ParseSchemaVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Digits only: no sign, no spaces, so that what is written is what is read.
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ParseSchemaVersionError);
        }
        s.parse::<u32>()
            .map(SchemaVersion)
            .map_err(|_| ParseSchemaVersionError)
    }
}

impl serde::Serialize for SchemaVersion {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u32(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for SchemaVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        u32::deserialize(d).map(SchemaVersion)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn parses_plain_integers_only() {
        assert_eq!("3".parse(), Ok(SchemaVersion(3)));
        for bad in ["", "-1", "+1", " 1", "1 ", "1.0", "x", "99999999999"] {
            assert!(bad.parse::<SchemaVersion>().is_err(), "{bad:?}");
        }
    }

    proptest! {
        #[test]
        fn display_then_parse_round_trips(n in any::<u32>()) {
            let v = SchemaVersion(n);
            prop_assert_eq!(v.to_string().parse::<SchemaVersion>(), Ok(v));
        }

        #[test]
        fn parsing_never_panics(s in ".*") {
            let _ = s.parse::<SchemaVersion>();
        }
    }
}
