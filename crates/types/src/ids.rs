// SPDX-License-Identifier: GPL-3.0-or-later
//! Random identifiers written as lowercase hexadecimal (design note 001 §5.2, note 002 §6.2).

use std::fmt;
use std::str::FromStr;

/// The text is not a valid identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdParseError {
    /// The text does not have the length of this kind of identifier.
    #[error("expected {expected} hexadecimal characters, found {found}")]
    Length {
        /// The number of characters expected.
        expected: usize,
        /// The number of characters found.
        found: usize,
    },
    /// A character is not a lowercase hexadecimal digit.
    #[error("identifiers are lowercase hexadecimal; found {0:?}")]
    NotHex(char),
    /// A member reference has more parts than a photo and a version.
    #[error("not a member reference: {0:?}")]
    Reference(String),
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 15) as usize] as char);
    }
    s
}

pub(crate) fn from_hex<const N: usize>(text: &str) -> Result<[u8; N], IdParseError> {
    if text.len() != N * 2 {
        return Err(IdParseError::Length {
            expected: N * 2,
            found: text.chars().count(),
        });
    }
    let mut out = [0u8; N];
    let bytes = text.as_bytes();
    for (i, pair) in bytes.chunks(2).enumerate() {
        let digit = |c: u8| match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            _ => Err(IdParseError::NotHex(c as char)),
        };
        out[i] = (digit(pair[0])? << 4) | digit(pair[1])?;
    }
    Ok(out)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    to_hex(bytes)
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("the operating system provides random bytes");
    b
}

macro_rules! hex_id {
    ($(#[$doc:meta])* $name:ident, $len:expr) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; $len]);

        impl $name {
            /// The number of bytes of this kind of identifier.
            pub const BYTES: usize = $len;

            /// A new random identifier.
            pub fn random() -> Self {
                Self(random_bytes())
            }

            /// An identifier from its bytes.
            pub const fn from_bytes(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }

            /// The bytes of the identifier.
            pub const fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&to_hex(&self.0))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), to_hex(&self.0))
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                from_hex::<$len>(s).map(Self)
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

hex_id!(
    /// The identifier of a photo: 128 random bits, 32 characters. Immutable (note 001 §5.2).
    PhotoId,
    16
);
hex_id!(
    /// The identifier of a workspace: 128 random bits (note 001 §5.3).
    WorkspaceId,
    16
);
hex_id!(
    /// The identifier of a version: 64 random bits, 16 characters (note 001 §5.2).
    VersionId,
    8
);
hex_id!(
    /// The identifier of a keyword of the vocabulary: 64 random bits (note 002 §6.2).
    KeywordId,
    8
);
hex_id!(
    /// The identifier of a collection: 64 random bits (note 002 §6.2).
    CollectionId,
    8
);
hex_id!(
    /// The identifier of a series: 64 random bits (note 002 §6.2).
    SeriesId,
    8
);
hex_id!(
    /// The identifier of a source: 64 random bits (note 002 §6.2).
    SourceId,
    8
);
hex_id!(
    /// The identifier of a publication: 64 random bits (note 002 §6.2).
    PublicationId,
    8
);

impl PhotoId {
    /// The shard folder of this photo: its first two characters (note 001 §5.1).
    pub fn shard(&self) -> String {
        to_hex(&self.0[..1])
    }
}

/// A reference to a photo, or to one of its versions, as written in collections and series
/// (note 002 §6.2): `<photo id>` or `<photo id>.<version id>`, the stem of the sidecar's name.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum MemberRef {
    /// A photo.
    Photo(PhotoId),
    /// A version of a photo.
    Version(PhotoId, VersionId),
}

impl MemberRef {
    /// The photo this reference is about.
    pub fn photo(&self) -> PhotoId {
        match self {
            Self::Photo(p) | Self::Version(p, _) => *p,
        }
    }
}

impl fmt::Display for MemberRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Photo(p) => write!(f, "{p}"),
            Self::Version(p, v) => write!(f, "{p}.{v}"),
        }
    }
}

impl FromStr for MemberRef {
    type Err = IdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('.');
        let photo = parts.next().unwrap_or_default().parse()?;
        match (parts.next(), parts.next()) {
            (None, _) => Ok(Self::Photo(photo)),
            (Some(v), None) => Ok(Self::Version(photo, v.parse()?)),
            _ => Err(IdParseError::Reference(s.to_string())),
        }
    }
}

impl serde::Serialize for MemberRef {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for MemberRef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn lengths_and_shape() {
        let p = PhotoId::random();
        assert_eq!(p.to_string().len(), 32);
        assert_eq!(VersionId::random().to_string().len(), 16);
        assert_eq!(p.shard().len(), 2);
        assert!(p.to_string().starts_with(&p.shard()));
        assert_ne!(PhotoId::random(), PhotoId::random());
    }

    #[test]
    fn only_lowercase_hex_of_the_right_length() {
        assert!(
            "3f2a91c0d77e4b5a8c1e0f9d2b6a4c31"
                .parse::<PhotoId>()
                .is_ok()
        );
        assert!(
            "3F2A91C0D77E4B5A8C1E0F9D2B6A4C31"
                .parse::<PhotoId>()
                .is_err()
        );
        assert!("3f2a".parse::<PhotoId>().is_err());
        assert!(
            "zz2a91c0d77e4b5a8c1e0f9d2b6a4c31"
                .parse::<PhotoId>()
                .is_err()
        );
        assert!("".parse::<VersionId>().is_err());
    }

    #[test]
    fn member_references() {
        let p = "3f2a91c0d77e4b5a8c1e0f9d2b6a4c31";
        let v = "7be1a0c4d2f95e08";
        assert_eq!(format!("{}", p.parse::<MemberRef>().unwrap()), p);
        let both = format!("{p}.{v}");
        assert_eq!(both.parse::<MemberRef>().unwrap().to_string(), both);
        assert!(format!("{p}.{v}.x").parse::<MemberRef>().is_err());
    }

    #[test]
    fn serde_uses_the_hex_text() {
        let p = PhotoId::from_bytes([0xab; 16]);
        let text = serde_json::to_string(&p).unwrap();
        assert_eq!(text, format!("\"{}\"", "ab".repeat(16)));
        assert_eq!(serde_json::from_str::<PhotoId>(&text).unwrap(), p);
    }

    proptest! {
        #[test]
        fn photo_ids_round_trip(bytes in any::<[u8; 16]>()) {
            let id = PhotoId::from_bytes(bytes);
            prop_assert_eq!(id.to_string().parse::<PhotoId>(), Ok(id));
        }

        #[test]
        fn parsing_never_panics(s in ".*") {
            let _ = s.parse::<PhotoId>();
            let _ = s.parse::<MemberRef>();
        }
    }
}
