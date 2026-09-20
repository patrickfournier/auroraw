//! Spike 3: a SQLite catalogue of 100,000 photos, its thumbnails and its workspace of sidecars.
//! Throwaway code that measures whether the design of the specification meets its budgets.

pub mod dataset;

use std::path::PathBuf;

/// Where the generated data goes. Large, so not in the repository.
pub fn data_dir() -> PathBuf {
    std::env::var("AUR_DATA").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join("auroraw-spike3"))
}

pub const SCHEMA: &str = include_str!("schema.sql");
pub const INDEXES: &str = include_str!("indexes.sql");

pub fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

pub fn pct(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() as f64 * q) as usize).min(s.len() - 1)]
}

/// Asks the kernel to drop a file's pages from the cache, so the next read comes from disk.
#[cfg(unix)]
pub fn evict(path: &std::path::Path) {
    use std::os::unix::io::AsRawFd;
    if let Ok(f) = std::fs::File::open(path) {
        unsafe {
            libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
        }
    }
}
#[cfg(not(unix))]
pub fn evict(_path: &std::path::Path) {}
