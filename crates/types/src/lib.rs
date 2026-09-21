// SPDX-License-Identifier: GPL-3.0-or-later
//! Identifiers, versions and small value types shared by every crate.
//!
//! The identifiers follow design notes 001 (workspace layout), 002 (state files) and 004
//! (fingerprint): random, written as lowercase hexadecimal, immutable.

mod digest;
mod ids;
mod schema;
mod time_stamp;

pub use digest::{ContentHash, DigestParseError, Fingerprint};
pub use ids::{
    CollectionId, IdParseError, KeywordId, MemberRef, PhotoId, PublicationId, SeriesId, SourceId,
    VersionId, WorkspaceId,
};
pub use schema::{ParseSchemaVersionError, SchemaVersion};
pub use time_stamp::{Timestamp, TimestampParseError};

/// The application name.
pub const APP_NAME: &str = "Auroraw";

/// The application version, from the crate metadata.
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!app_version().is_empty());
    }
}
