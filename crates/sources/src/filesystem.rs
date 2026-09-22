// SPDX-License-Identifier: GPL-3.0-or-later
//! A source backed by a plain folder on disk: a local folder or a removable volume's mount
//! point, which need no different implementation once they are mounted (architecture §12: card
//! detection differs by platform, but reading a mounted card does not). Never writes (D-018).

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use auroraw_plugin_api::source::{Entry, Source, SourceError, SourceState, Watch};

use crate::relink::FoundFile;

/// `sources.kind` for a folder the photographer pointed at directly.
pub const LOCAL_FOLDER: &str = "local-folder";
/// `sources.kind` for a folder recognised as a removable volume's mount point.
pub const REMOVABLE_VOLUME: &str = "removable-volume";

/// A folder on this machine, read through [`Source`]. Used for both `LOCAL_FOLDER` and
/// `REMOVABLE_VOLUME` sources: what distinguishes them is how they were found (a person picked a
/// path, or [`crate::volumes::list_removable_volumes`] found a mounted card), not how they are
/// read.
#[derive(Debug, Clone)]
pub struct FilesystemSource {
    root: PathBuf,
}

impl FilesystemSource {
    /// A source rooted at `root`. Nothing is read yet: [`Source::state`] checks the root lazily,
    /// every time, since a folder can appear or vanish (a card, a network mount) between calls.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The root this source reads from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// One file's sampled fingerprint (design note 004 §6.1). Reads the real file directly
    /// rather than through [`Source::read_range`]: every M1 source is a folder on this machine,
    /// so there is no network round trip to spare, and going through the trait would mean
    /// re-deriving `auroraw_format::fingerprint`'s exact sampling in terms of byte ranges instead
    /// of calling the one, tested implementation on a real reader.
    pub fn fingerprint_of(&self, path: &str) -> Result<auroraw_types::Fingerprint, SourceError> {
        let full = self.full_path(path)?;
        let mut file = fs::File::open(&full).map_err(|_| SourceError::NotFound {
            path: path.to_string(),
        })?;
        let (_size, fingerprint) = auroraw_format::fingerprint::fingerprint(&mut file)
            .map_err(|e| SourceError::Other(e.to_string()))?;
        Ok(fingerprint)
    }

    /// Every file, fingerprinted, for [`crate::relink::reconcile`].
    pub fn scan(&self) -> Result<Vec<FoundFile>, SourceError> {
        self.list()?
            .into_iter()
            .map(|entry| {
                let fingerprint = self.fingerprint_of(&entry.path)?;
                Ok(FoundFile {
                    path: entry.path,
                    fingerprint,
                })
            })
            .collect()
    }

    fn full_path(&self, path: &str) -> Result<PathBuf, SourceError> {
        if path.starts_with('/') || path.split('/').any(|part| part == "..") {
            return Err(SourceError::Other(format!("{path:?} escapes the source")));
        }
        Ok(self.root.join(path))
    }

    fn entry_of(path: String, meta: &fs::Metadata) -> Entry {
        Entry {
            path,
            size: meta.len(),
            modified: meta.modified().ok(),
        }
    }
}

fn walk(dir: &Path, root: &Path, out: &mut Vec<Entry>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = entry.metadata()?;
        if meta.is_dir() {
            walk(&path, root, out)?;
        } else if meta.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            out.push(FilesystemSource::entry_of(relative, &meta));
        }
    }
    Ok(())
}

impl Source for FilesystemSource {
    fn state(&self) -> SourceState {
        match fs::metadata(&self.root) {
            Ok(meta) if meta.is_dir() => SourceState::Online,
            Ok(_) => SourceState::Missing, // a file where a folder is expected: treat as gone
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => SourceState::Missing,
            Err(_) => SourceState::Offline, // exists but unreadable right now (permissions, a share hiccup)
        }
    }

    fn writable(&self) -> bool {
        fs::metadata(&self.root)
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false)
    }

    fn list(&self) -> Result<Vec<Entry>, SourceError> {
        if self.state() != SourceState::Online {
            return Err(SourceError::Unreachable);
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out).map_err(|e| SourceError::Other(e.to_string()))?;
        Ok(out)
    }

    fn stat(&self, path: &str) -> Result<Entry, SourceError> {
        let full = self.full_path(path)?;
        let meta = fs::metadata(&full).map_err(|_| SourceError::NotFound {
            path: path.to_string(),
        })?;
        Ok(Self::entry_of(path.to_string(), &meta))
    }

    fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, SourceError> {
        let full = self.full_path(path)?;
        let mut file = fs::File::open(&full).map_err(|_| SourceError::NotFound {
            path: path.to_string(),
        })?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| SourceError::Other(e.to_string()))?;
        let mut buf = vec![0u8; len as usize];
        let mut read = 0;
        while read < buf.len() {
            let n = file
                .read(&mut buf[read..])
                .map_err(|e| SourceError::Other(e.to_string()))?;
            if n == 0 {
                buf.truncate(read);
                break;
            }
            read += n;
        }
        Ok(buf)
    }

    fn watch(&self, on_change: Sender<()>) -> Result<Option<Watch>, SourceError> {
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if res.is_ok() {
                let _ = on_change.send(());
            }
        })
        .map_err(|e| SourceError::Other(e.to_string()))?;
        use notify::Watcher;
        watcher
            .watch(&self.root, notify::RecursiveMode::Recursive)
            .map_err(|e| SourceError::Other(e.to_string()))?;
        Ok(Some(Watch(Box::new(watcher))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auroraw_testkit::temp_dir;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn a_missing_root_is_missing_not_online() {
        let dir = temp_dir();
        let src = FilesystemSource::new(dir.path().join("nope"));
        assert_eq!(src.state(), SourceState::Missing);
        assert!(matches!(src.list(), Err(SourceError::Unreachable)));
    }

    #[test]
    fn list_finds_files_in_subfolders_with_forward_slash_paths() {
        let dir = temp_dir();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        fs::write(dir.path().join("top.jpg"), b"top").unwrap();
        fs::write(dir.path().join("a/b/nested.jpg"), b"nested").unwrap();
        let src = FilesystemSource::new(dir.path());
        assert_eq!(src.state(), SourceState::Online);
        let mut paths: Vec<_> = src.list().unwrap().into_iter().map(|e| e.path).collect();
        paths.sort();
        assert_eq!(
            paths,
            vec!["a/b/nested.jpg".to_string(), "top.jpg".to_string()]
        );
    }

    #[test]
    fn stat_and_read_range_agree_with_the_file() {
        let dir = temp_dir();
        fs::write(dir.path().join("photo.raw"), b"0123456789").unwrap();
        let src = FilesystemSource::new(dir.path());
        let entry = src.stat("photo.raw").unwrap();
        assert_eq!(entry.size, 10);
        assert_eq!(src.read_range("photo.raw", 3, 4).unwrap(), b"3456");
    }

    #[test]
    fn a_path_that_escapes_the_root_is_refused() {
        let dir = temp_dir();
        let src = FilesystemSource::new(dir.path());
        assert!(src.stat("../outside").is_err());
        assert!(src.stat("/etc/passwd").is_err());
    }

    #[test]
    fn watching_reports_a_new_file() {
        let dir = temp_dir();
        let src = FilesystemSource::new(dir.path());
        let (tx, rx) = mpsc::channel();
        let _watch = src
            .watch(tx)
            .unwrap()
            .expect("a local folder can be watched");
        fs::write(dir.path().join("new.jpg"), b"x").unwrap();
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "expected a change notification"
        );
    }
}
