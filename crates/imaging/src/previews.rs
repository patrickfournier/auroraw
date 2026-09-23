// SPDX-License-Identifier: GPL-3.0-or-later
//! The previews database (D-075, architecture §5.7): one small SQLite database of thumbnail
//! blobs per catalogue, 32 KB pages (spike 3: four times faster than 4 KB for the default view,
//! the grid's most common access pattern), in the cache -- a pure cache, deletable at any time
//! without losing anything, regenerated from the workspace and the sources. This module does not
//! resolve the cache directory itself (`app`'s job, matching `catalogue::registry`'s own
//! precedent): it opens whatever path its caller gives it.

use std::path::Path;
use std::time::Duration;

use auroraw_types::PhotoId;
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::Result;
use crate::thumbnail::Thumbnail;

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS preview(
    photo_id TEXT PRIMARY KEY,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    jpeg BLOB NOT NULL
);";

/// An open previews database.
pub struct PreviewsDb {
    conn: Connection,
}

impl PreviewsDb {
    /// Opens (creating if needed) the previews database at `path`. `PRAGMA page_size` only takes
    /// effect on a brand new file; reopening an existing one keeps whatever size it already has.
    /// WAL and a busy timeout (matching `catalogue::open`'s own reasoning, D-073): several
    /// thumbnail worker threads (WP8) open their own connection to this same file and both read
    /// and write it concurrently, which the rollback-journal default handles only by blocking
    /// (and, past `busy_timeout`, failing) one writer behind another.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "page_size", 32768)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// An in-memory database, for tests: no file, no page size to speak of.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Stores (or replaces) a photo's thumbnail.
    pub fn put(&self, photo_id: &PhotoId, thumbnail: &Thumbnail) -> Result<()> {
        self.conn.execute(
            "INSERT INTO preview(photo_id, width, height, jpeg) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(photo_id) DO UPDATE SET width = excluded.width, height = excluded.height, jpeg = excluded.jpeg",
            params![photo_id.to_string(), thumbnail.width, thumbnail.height, thumbnail.jpeg],
        )?;
        Ok(())
    }

    /// The stored thumbnail, if there is one.
    pub fn get(&self, photo_id: &PhotoId) -> Result<Option<Thumbnail>> {
        Ok(self
            .conn
            .query_row(
                "SELECT width, height, jpeg FROM preview WHERE photo_id = ?1",
                [photo_id.to_string()],
                |r| {
                    Ok(Thumbnail {
                        width: r.get(0)?,
                        height: r.get(1)?,
                        jpeg: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// Removes a photo's thumbnail, if any (its source file went missing, or the photo itself
    /// did): never an error either way, matching a cache's own "deletable at will".
    pub fn remove(&self, photo_id: &PhotoId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM preview WHERE photo_id = ?1",
            [photo_id.to_string()],
        )?;
        Ok(())
    }

    /// How many thumbnails are stored.
    pub fn count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM preview", [], |r| r.get::<_, i64>(0))?
            as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thumb() -> Thumbnail {
        Thumbnail {
            jpeg: vec![1, 2, 3],
            width: 256,
            height: 171,
        }
    }

    #[test]
    fn put_then_get_round_trips() {
        let db = PreviewsDb::open_in_memory().unwrap();
        let id = PhotoId::random();
        assert!(db.get(&id).unwrap().is_none());
        db.put(&id, &thumb()).unwrap();
        assert_eq!(db.get(&id).unwrap(), Some(thumb()));
        assert_eq!(db.count().unwrap(), 1);
    }

    #[test]
    fn put_twice_replaces_not_duplicates() {
        let db = PreviewsDb::open_in_memory().unwrap();
        let id = PhotoId::random();
        db.put(&id, &thumb()).unwrap();
        let bigger = Thumbnail {
            width: 300,
            ..thumb()
        };
        db.put(&id, &bigger).unwrap();
        assert_eq!(db.get(&id).unwrap(), Some(bigger));
        assert_eq!(db.count().unwrap(), 1);
    }

    #[test]
    fn remove_is_never_an_error_even_if_nothing_was_there() {
        let db = PreviewsDb::open_in_memory().unwrap();
        let id = PhotoId::random();
        db.remove(&id).unwrap();
        db.put(&id, &thumb()).unwrap();
        db.remove(&id).unwrap();
        assert!(db.get(&id).unwrap().is_none());
    }

    #[test]
    fn opening_the_same_file_twice_keeps_what_was_stored() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("previews.sqlite");
        let id = PhotoId::random();
        {
            let db = PreviewsDb::open(&path).unwrap();
            db.put(&id, &thumb()).unwrap();
        }
        let reopened = PreviewsDb::open(&path).unwrap();
        assert_eq!(reopened.get(&id).unwrap(), Some(thumb()));
    }
}
