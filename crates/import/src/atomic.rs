// SPDX-License-Identifier: GPL-3.0-or-later
//! A small write-through-a-temporary-file-and-rename helper (design note 001 §5.4's pattern).
//! This crate does not depend on `workspace` (whose own version of this is crate-private, and
//! whose canonical-JSON envelope is for synced state files, not this crate's own profiles and
//! per-job import state, or the copied photos themselves), so it keeps a minimal copy.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Writes `bytes` to `target` through a temporary file next to it, so a crash leaves the old
/// file (or nothing) rather than a truncated one. Overwrites a stale temporary file left by an
/// earlier, interrupted write to this same `target`: unlike a synced state file, where two
/// writers could otherwise collide, only one import job ever writes a given target.
pub(crate) fn write(target: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = target.with_extension(match target.extension() {
        Some(ext) => format!("{}.part", ext.to_string_lossy()),
        None => "part".to_string(),
    });
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_creates_parent_folders() {
        let dir = auroraw_testkit::temp_dir();
        let target = dir.path().join("a/b/c.bin");
        write(&target, b"hello").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"hello");
    }

    #[test]
    fn a_stale_temporary_file_from_an_earlier_attempt_is_overwritten() {
        let dir = auroraw_testkit::temp_dir();
        let target = dir.path().join("c.bin");
        fs::write(dir.path().join("c.part"), b"stale, half-written").unwrap();
        write(&target, b"fresh").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"fresh");
    }

    #[test]
    fn overwrites_an_existing_target() {
        let dir = auroraw_testkit::temp_dir();
        let target = dir.path().join("c.bin");
        write(&target, b"old").unwrap();
        write(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
    }
}
