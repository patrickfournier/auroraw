// SPDX-License-Identifier: GPL-3.0-or-later
//! Test helpers, used by other crates in their `dev-dependencies` only (testing strategy §1, §5).
//! Every test is hermetic and deterministic: its own temporary directory, fixed random seeds.

/// A fresh temporary directory, removed when dropped.
pub fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary directory")
}

/// A random generator with a fixed seed, so a failure can be replayed.
pub fn rng(seed: u64) -> fastrand::Rng {
    fastrand::Rng::with_seed(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_dirs_are_distinct_and_removed() {
        let (a, b) = (temp_dir(), temp_dir());
        assert_ne!(a.path(), b.path());
        let gone = a.path().to_path_buf();
        drop(a);
        assert!(!gone.exists());
    }

    #[test]
    fn the_same_seed_gives_the_same_numbers() {
        let (mut x, mut y) = (rng(7), rng(7));
        assert_eq!(
            (0..8).map(|_| x.u64(..)).collect::<Vec<_>>(),
            (0..8).map(|_| y.u64(..)).collect::<Vec<_>>()
        );
    }
}
