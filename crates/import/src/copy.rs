// SPDX-License-Identifier: GPL-3.0-or-later
//! Verified copy (spec §5.2, design note 004 §6.3 items 4-5): read the whole file from the
//! source once, computing its fingerprint and whole-file hash on the way (free: the read already
//! happened for the copy), write it to every destination, and read each one back to prove it
//! matches before trusting it. Never touches the source beyond reading it (D-018, D-031).
//!
//! Split into [`read_source`] and [`write_verified`], not one function, because the skip decision
//! (design note 004 §6.3, item 4) sits between them: the file must be read and hashed before
//! anything is known about whether it needs to be written at all, and the whole point of the rule
//! is that a fingerprint match alone never causes a file to be skipped -- only a hash match does,
//! and that hash only exists once the file has been read.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use auroraw_format::fingerprint::{content_hash, fingerprint};
use auroraw_plugin_api::source::Source;
use auroraw_types::{ContentHash, Fingerprint};

use crate::error::{ImportError, Result};

/// A file read whole from a source, with its fingerprint and whole-file hash already computed.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceRead {
    /// The file's bytes.
    pub bytes: Vec<u8>,
    /// Its size (`bytes.len()` as `u64`).
    pub size: u64,
    /// Its sampled fingerprint.
    pub fingerprint: Fingerprint,
    /// Its whole-file hash.
    pub hash: ContentHash,
}

/// What copying and verifying one file produced.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyOutcome {
    /// The file's size.
    pub size: u64,
    /// Its sampled fingerprint.
    pub fingerprint: Fingerprint,
    /// Its whole-file hash.
    pub hash: ContentHash,
}

/// Reads the whole file at `source_path` in `source`, computing its fingerprint and hash.
pub fn read_source(source: &dyn Source, source_path: &str) -> Result<SourceRead> {
    let stat = source.stat(source_path).map_err(|e| ImportError::Source {
        path: source_path.to_string(),
        source: e,
    })?;
    let bytes = source
        .read_range(source_path, 0, stat.size)
        .map_err(|e| ImportError::Source {
            path: source_path.to_string(),
            source: e,
        })?;
    let mut cursor = Cursor::new(&bytes);
    // `fingerprint` rewinds the cursor to the start on success, so `content_hash` right after
    // hashes the whole file, not whatever the last sampled chunk left behind.
    let (size, fp) =
        fingerprint(&mut cursor).map_err(|e| ImportError::io(Path::new(source_path), e))?;
    let (_, hash) =
        content_hash(&mut cursor).map_err(|e| ImportError::io(Path::new(source_path), e))?;
    Ok(SourceRead {
        bytes,
        size,
        fingerprint: fp,
        hash,
    })
}

/// Writes `bytes` to `destination` and every path in `backups`, verifying each one by reading it
/// back after a flush and comparing its whole-file hash with `hash` (design note 004 §6.3, item
/// 5). Every destination must verify for this to succeed: a resume retries the whole file rather
/// than trying to patch up one bad destination among several.
pub fn write_verified(
    bytes: &[u8],
    hash: &ContentHash,
    destination: &Path,
    backups: &[PathBuf],
) -> Result<()> {
    for dest in std::iter::once(destination).chain(backups.iter().map(PathBuf::as_path)) {
        write_and_verify(dest, bytes, hash)?;
    }
    Ok(())
}

/// Reads and writes in one call, for callers that never need to decide whether to skip in
/// between (most tests, and any copy this crate's own callers do unconditionally).
pub fn copy_verified(
    source: &dyn Source,
    source_path: &str,
    destination: &Path,
    backups: &[PathBuf],
) -> Result<CopyOutcome> {
    let read = read_source(source, source_path)?;
    write_verified(&read.bytes, &read.hash, destination, backups)?;
    Ok(CopyOutcome {
        size: read.size,
        fingerprint: read.fingerprint,
        hash: read.hash,
    })
}

fn write_and_verify(dest: &Path, bytes: &[u8], expected: &ContentHash) -> Result<()> {
    crate::atomic::write(dest, bytes).map_err(|e| ImportError::io(dest, e))?;
    let written = std::fs::read(dest).map_err(|e| ImportError::io(dest, e))?;
    let (_, actual) =
        content_hash(&mut Cursor::new(&written)).map_err(|e| ImportError::io(dest, e))?;
    if actual != *expected {
        return Err(ImportError::VerificationFailed {
            path: dest.to_path_buf(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use auroraw_sources::fake::FakeSource;

    #[test]
    fn a_copy_matches_the_source_and_verifies() {
        let dir = auroraw_testkit::temp_dir();
        let source = FakeSource::new();
        source.put("a.raw", b"the pixels".to_vec());

        let dest = dir.path().join("archive/a.raw");
        let outcome = copy_verified(&source, "a.raw", &dest, &[]).unwrap();

        assert_eq!(outcome.size, 10);
        assert_eq!(std::fs::read(&dest).unwrap(), b"the pixels");
        let (_, expected) = content_hash(&mut Cursor::new(b"the pixels")).unwrap();
        assert_eq!(outcome.hash, expected);
    }

    #[test]
    fn backup_destinations_get_the_same_verified_bytes() {
        let dir = auroraw_testkit::temp_dir();
        let source = FakeSource::new();
        source.put("a.raw", b"backed up".to_vec());

        let dest = dir.path().join("archive/a.raw");
        let backup = dir.path().join("backup/a.raw");
        copy_verified(&source, "a.raw", &dest, std::slice::from_ref(&backup)).unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), b"backed up");
        assert_eq!(std::fs::read(&backup).unwrap(), b"backed up");
    }

    #[test]
    fn a_missing_source_file_is_reported_not_panicked() {
        let dir = auroraw_testkit::temp_dir();
        let source = FakeSource::new();
        let dest = dir.path().join("a.raw");
        assert!(matches!(
            copy_verified(&source, "missing.raw", &dest, &[]),
            Err(ImportError::Source { .. })
        ));
    }

    #[test]
    fn reading_then_deciding_then_writing_composes_the_same_as_copy_verified() {
        let dir = auroraw_testkit::temp_dir();
        let source = FakeSource::new();
        source.put("a.raw", b"split path".to_vec());
        let dest = dir.path().join("a.raw");

        let read = read_source(&source, "a.raw").unwrap();
        write_verified(&read.bytes, &read.hash, &dest, &[]).unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), b"split path");
    }
}
