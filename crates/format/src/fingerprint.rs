// SPDX-License-Identifier: GPL-3.0-or-later
//! The sampled fingerprint and the whole-file hash (design note 004 §6.1).
//!
//! The fingerprint is BLAKE3 over the text `auroraw-sampled-v1`, the size as 8 bytes little-endian,
//! then 64 KB from the start, 64 KB from the middle (at `size / 2` rounded down to a multiple of
//! 4,096) and 64 KB from the end. A file of 192 KB or less is read whole. The hash is BLAKE3 of the
//! whole file.
//!
//! Both functions take a generic reader, not a path: neither knows what underlies it (a real
//! file, a network share, a buffer in a test), so neither can name it in an error. They return a
//! bare `std::io::Error` and let it propagate; the caller, who opened the reader and knows its
//! path, is the one place that can usefully attach that context, exactly as `WorkspaceError::Io`
//! and `CatalogueError::Io` already do for the files those crates open themselves. A short read
//! (the file truncated while being read, for instance) surfaces the same way, as an
//! `UnexpectedEof` from the failing `read_exact` or `seek`, never as a wrong fingerprint.

use std::io::{self, Read, Seek, SeekFrom};

use auroraw_types::{ContentHash, Fingerprint};

/// The size of each sampled chunk.
pub const SAMPLE: u64 = 64 * 1024;

const DOMAIN: &[u8] = b"auroraw-sampled-v1";

fn read_chunk<R: Read + Seek>(
    r: &mut R,
    offset: u64,
    len: u64,
    hasher: &mut blake3::Hasher,
) -> io::Result<()> {
    r.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    hasher.update(&buf);
    Ok(())
}

/// The size and the sampled fingerprint of a file. On success, `r` is left positioned at the
/// start: calling [`content_hash`] on the same reader right afterwards hashes the whole file, not
/// whatever the last sampled chunk happened to leave behind. On error, its position is
/// unspecified, matching `Read`'s own convention for a failed read.
pub fn fingerprint<R: Read + Seek>(r: &mut R) -> io::Result<(u64, Fingerprint)> {
    let size = r.seek(SeekFrom::End(0))?;
    let mut h = blake3::Hasher::new();
    h.update(DOMAIN);
    h.update(&size.to_le_bytes());
    if size <= 3 * SAMPLE {
        read_chunk(r, 0, size, &mut h)?;
    } else {
        read_chunk(r, 0, SAMPLE, &mut h)?;
        read_chunk(r, (size / 2) & !4095, SAMPLE, &mut h)?;
        read_chunk(r, size - SAMPLE, SAMPLE, &mut h)?;
    }
    r.seek(SeekFrom::Start(0))?;
    Ok((size, Fingerprint::from_bytes(*h.finalize().as_bytes())))
}

/// The size and the whole-file hash of a file, read from the current position to the end (so
/// calling it right after [`fingerprint`], which rewinds first, hashes the whole file). Leaves
/// `r` positioned at the end on success.
pub fn content_hash<R: Read>(r: &mut R) -> io::Result<(u64, ContentHash)> {
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        size += n as u64;
    }
    Ok((size, ContentHash::from_bytes(*h.finalize().as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A deterministic byte pattern that differs everywhere.
    fn pattern(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i.wrapping_mul(31).wrapping_add(i >> 8)) as u8)
            .collect()
    }

    /// The definition of note 004, built independently from the raw bytes.
    fn expected(data: &[u8]) -> Fingerprint {
        let mut h = blake3::Hasher::new();
        h.update(b"auroraw-sampled-v1");
        h.update(&(data.len() as u64).to_le_bytes());
        let s = SAMPLE as usize;
        if data.len() <= 3 * s {
            h.update(data);
        } else {
            let mid = (data.len() / 2) & !4095;
            h.update(&data[..s]);
            h.update(&data[mid..mid + s]);
            h.update(&data[data.len() - s..]);
        }
        Fingerprint::from_bytes(*h.finalize().as_bytes())
    }

    #[test]
    fn matches_the_definition_at_the_edges_of_the_rule() {
        for len in [
            0,
            1,
            4095,
            65_536,
            196_607,
            196_608,
            196_609,
            500_000,
            1_048_576 + 17,
        ] {
            let data = pattern(len);
            let (size, fp) = fingerprint(&mut Cursor::new(&data)).unwrap();
            assert_eq!(size, len as u64);
            assert_eq!(fp, expected(&data), "length {len}");
        }
    }

    #[test]
    fn fingerprint_rewinds_so_content_hash_can_follow_it_on_the_same_reader() {
        let data = pattern(1_000_000);
        let mut cursor = Cursor::new(&data);
        let (size, _fp) = fingerprint(&mut cursor).unwrap();
        assert_eq!(
            cursor.position(),
            0,
            "left at the start, not wherever the last sample ended"
        );
        let (hashed_size, hash) = content_hash(&mut cursor).unwrap();
        assert_eq!(
            hashed_size, size,
            "the whole file, not the tail left by fingerprint"
        );
        assert_eq!(
            hash,
            content_hash(&mut Cursor::new(&data)).unwrap().1,
            "same hash as a fresh reader"
        );
    }

    #[test]
    fn known_answers_never_change() {
        // If these change, every recorded fingerprint in every workspace is invalid.
        let (_, fp) = fingerprint(&mut Cursor::new(pattern(1_000_000))).unwrap();
        assert_eq!(fp.to_string(), KNOWN_FINGERPRINT);
        let (size, h) = content_hash(&mut Cursor::new(pattern(1_000_000))).unwrap();
        assert_eq!(size, 1_000_000);
        assert_eq!(h.to_string(), KNOWN_HASH);
    }

    #[test]
    fn a_change_outside_the_samples_keeps_the_fingerprint_but_not_the_hash() {
        // The documented limit of the fingerprint (note 004 §7): this is why import skips and copy
        // verification rest on the whole-file hash.
        let a = pattern(1_000_000);
        let mut b = a.clone();
        b[300_000] ^= 0xff; // not in the first, middle (500,000) or last 64 KB
        let fa = fingerprint(&mut Cursor::new(&a)).unwrap().1;
        let fb = fingerprint(&mut Cursor::new(&b)).unwrap().1;
        assert_eq!(fa, fb);
        assert_ne!(
            content_hash(&mut Cursor::new(&a)).unwrap().1,
            content_hash(&mut Cursor::new(&b)).unwrap().1
        );
    }

    #[test]
    fn a_change_inside_a_sample_changes_the_fingerprint() {
        let a = pattern(1_000_000);
        for at in [0, 65_535, 499_712, 500_000, 999_999, 1_000_000 - 65_536] {
            let mut b = a.clone();
            b[at] ^= 1;
            assert_ne!(
                fingerprint(&mut Cursor::new(&a)).unwrap().1,
                fingerprint(&mut Cursor::new(&b)).unwrap().1,
                "byte {at}"
            );
        }
    }

    const KNOWN_FINGERPRINT: &str =
        "sampled-v1:8d1f998a9fb5c3d99321443aec0a3fa10b73e8813dbe4abff1b3fde7860fc37f";
    const KNOWN_HASH: &str =
        "blake3:5276136dfdc38f2847aa23f898c083da5b925d0fc6ea782acceca18c0d780d0a";
}
