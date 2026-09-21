// SPDX-License-Identifier: GPL-3.0-or-later
//! The two values that identify the content of a file (design note 004 §6.1): a sampled
//! fingerprint and a whole-file hash, both 256 bits, tagged with their algorithm.

use std::fmt;
use std::str::FromStr;

use crate::ids::{IdParseError, from_hex, hex};

/// The text is not a fingerprint or a hash.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DigestParseError {
    /// The tag before the colon is not the expected one.
    #[error("expected the tag {expected:?}")]
    Tag {
        /// The tag expected.
        expected: &'static str,
    },
    /// The hexadecimal part is malformed.
    #[error(transparent)]
    Hex(#[from] IdParseError),
}

macro_rules! digest {
    ($(#[$doc:meta])* $name:ident, $tag:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 32]);

        impl $name {
            /// The tag written before the hexadecimal value.
            pub const TAG: &'static str = $tag;

            /// A value from its 32 bytes.
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            /// The 32 bytes.
            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}:{}", $tag, hex(&self.0))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self)
            }
        }

        impl FromStr for $name {
            type Err = DigestParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let rest = s
                    .strip_prefix($tag)
                    .and_then(|r| r.strip_prefix(':'))
                    .ok_or(DigestParseError::Tag { expected: $tag })?;
                Ok(Self(from_hex::<32>(rest)?))
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

digest!(
    /// The sampled fingerprint of a file, `sampled-v1:<hex>`: enough to find a file, never
    /// enough to conclude anything that can lose data (note 004).
    Fingerprint,
    "sampled-v1"
);
digest!(
    /// The BLAKE3 hash of a whole file, `blake3:<hex>` (note 004).
    ContentHash,
    "blake3"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tagged_text_round_trips() {
        let f = Fingerprint::from_bytes([7; 32]);
        assert_eq!(f.to_string(), format!("sampled-v1:{}", "07".repeat(32)));
        assert_eq!(f.to_string().parse::<Fingerprint>(), Ok(f));
        let h = ContentHash::from_bytes([255; 32]);
        assert_eq!(h.to_string().parse::<ContentHash>(), Ok(h));
    }

    #[test]
    fn the_tag_must_match() {
        let f = Fingerprint::from_bytes([1; 32]).to_string();
        assert!(f.parse::<ContentHash>().is_err());
        assert!("blake3:00".parse::<ContentHash>().is_err());
        assert!("".parse::<Fingerprint>().is_err());
    }
}
