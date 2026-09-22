// SPDX-License-Identifier: GPL-3.0-or-later
//! The queries the grid and the search panel need: keyset paging (never offset, so a page costs
//! the same at the start or the end of a 100,000-photo catalogue, spike 3), counts, a full-text
//! search, and a lookup by keyword. See `docs/spikes/03-catalogue-and-grid.md` for the budgets.

use auroraw_types::{KeywordId, PhotoId, SeriesId};
use rusqlite::{OptionalExtension, Row, params};

use crate::error::Result;
use crate::open::Catalogue;

/// One row of the grid: enough to display and sort a photo without a further lookup.
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoRow {
    /// The photo's identifier.
    pub id: PhotoId,
    /// Seconds since the Unix epoch (0 if the capture time could not be parsed).
    pub capture_time: i64,
    /// The photo's own rating.
    pub rating: u8,
    /// The effective rating (D-063): the main version's if it overrides one.
    pub effective_rating: u8,
    /// Whether the effective rating differs from the photo's own (the main version overrides it).
    pub rating_overridden: bool,
    /// The photo's own flag (0 none, 1 picked, 2 rejected).
    pub flag: u8,
    /// The effective flag.
    pub effective_flag: u8,
    /// The colour label.
    pub label: Option<String>,
    /// The title.
    pub title: Option<String>,
    /// The caption.
    pub caption: Option<String>,
    /// The original file's name.
    pub filename: String,
    /// The camera name, if known.
    pub camera: Option<String>,
    /// The lens name, if known.
    pub lens: Option<String>,
    /// The series this photo belongs to, if any.
    pub series_id: Option<SeriesId>,
    /// How many versions this photo has.
    pub version_count: u32,
}

const COLUMNS: &str = "
    p.id, p.capture_time, p.rating, p.effective_rating, p.rating_overridden, p.flag,
    p.effective_flag, p.label, p.title, p.caption, p.filename, c.name, l.name, p.series_id,
    p.version_count
";
const FROM: &str =
    "FROM photo p LEFT JOIN camera c ON c.id = p.camera_id LEFT JOIN lens l ON l.id = p.lens_id";

fn photo_row(row: &Row) -> rusqlite::Result<PhotoRow> {
    let id: String = row.get(0)?;
    let series_id: Option<String> = row.get(13)?;
    Ok(PhotoRow {
        id: id.parse().map_err(|_| {
            rusqlite::Error::InvalidColumnType(0, "id".into(), rusqlite::types::Type::Text)
        })?,
        capture_time: row.get(1)?,
        rating: row.get(2)?,
        effective_rating: row.get(3)?,
        rating_overridden: row.get::<_, i64>(4)? != 0,
        flag: row.get(5)?,
        effective_flag: row.get(6)?,
        label: row.get(7)?,
        title: row.get(8)?,
        caption: row.get(9)?,
        filename: row.get(10)?,
        camera: row.get(11)?,
        lens: row.get(12)?,
        series_id: series_id.and_then(|s| s.parse().ok()),
        version_count: row.get(14)?,
    })
}

/// A position in the keyset order (`capture_time` descending, `id` descending as the tie-break):
/// the last row of the previous page. Small and `Copy`, so a caller can hold one across loop
/// iterations without borrowing the page it came from.
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    /// The capture time of the last row seen.
    pub capture_time: i64,
    /// Its identifier.
    pub id: PhotoId,
}

impl PhotoRow {
    /// The cursor that continues the list right after this row.
    pub fn cursor(&self) -> Cursor {
        Cursor {
            capture_time: self.capture_time,
            id: self.id,
        }
    }
}

impl Catalogue {
    /// The most recent photos first (`capture_time` descending), the usual grid order. `after`
    /// continues from a previous page's last row.
    pub fn list_recent(&self, after: Option<Cursor>, limit: u32) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "SELECT {COLUMNS} {FROM} {} ORDER BY p.capture_time DESC, p.id DESC LIMIT ?",
            if after.is_some() {
                "WHERE (p.capture_time, p.id) < (?, ?)"
            } else {
                ""
            }
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = match after {
            Some(c) => {
                stmt.query_map(params![c.capture_time, c.id.to_string(), limit], photo_row)?
            }
            None => stmt.query_map(params![limit], photo_row)?,
        };
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// How many photos there are in total.
    pub fn count_all(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM photo", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Photos whose effective rating is at least `min`, most recent first.
    pub fn list_by_min_rating(
        &self,
        min: u8,
        after: Option<Cursor>,
        limit: u32,
    ) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "SELECT {COLUMNS} {FROM} WHERE p.effective_rating >= ?1 {} ORDER BY p.capture_time DESC, p.id DESC LIMIT ?",
            if after.is_some() {
                "AND (p.capture_time, p.id) < (?, ?)"
            } else {
                ""
            }
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = match after {
            Some(c) => stmt.query_map(
                params![min, c.capture_time, c.id.to_string(), limit],
                photo_row,
            )?,
            None => stmt.query_map(params![min, limit], photo_row)?,
        };
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// How many photos have at least `min` as their effective rating.
    pub fn count_by_min_rating(&self, min: u8) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM photo WHERE effective_rating >= ?1",
            [min],
            |r| r.get::<_, i64>(0),
        )? as u64)
    }

    /// Photos in a given camera, most recent first.
    pub fn list_by_camera(
        &self,
        camera: &str,
        after: Option<Cursor>,
        limit: u32,
    ) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "SELECT {COLUMNS} {FROM} WHERE c.name = ?1 {} ORDER BY p.capture_time DESC, p.id DESC LIMIT ?",
            if after.is_some() {
                "AND (p.capture_time, p.id) < (?, ?)"
            } else {
                ""
            }
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = match after {
            Some(c) => stmt.query_map(
                params![camera, c.capture_time, c.id.to_string(), limit],
                photo_row,
            )?,
            None => stmt.query_map(params![camera, limit], photo_row)?,
        };
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Photos carrying a keyword (or, with `include_descendants`, any keyword under it in the
    /// vocabulary tree, matched by path prefix), most recent first.
    pub fn list_by_keyword(
        &self,
        keyword: &KeywordId,
        include_descendants: bool,
        after: Option<Cursor>,
        limit: u32,
    ) -> Result<Vec<PhotoRow>> {
        let base = if include_descendants {
            format!(
                "SELECT {COLUMNS} {FROM}
                 JOIN photo_keyword pk ON pk.photo_id = p.id
                 JOIN keyword k ON k.id = pk.keyword_id
                 WHERE (k.id = ?1 OR k.path LIKE (SELECT path || '|%' FROM keyword WHERE id = ?1))"
            )
        } else {
            format!(
                "SELECT {COLUMNS} {FROM} JOIN photo_keyword pk ON pk.photo_id = p.id AND pk.keyword_id = ?1"
            )
        };
        let sql = format!(
            "{base} {} ORDER BY p.capture_time DESC, p.id DESC LIMIT ?",
            if after.is_some() {
                "AND (p.capture_time, p.id) < (?, ?)"
            } else {
                ""
            }
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let text = keyword.to_string();
        let rows = match after {
            Some(c) => stmt.query_map(
                params![text, c.capture_time, c.id.to_string(), limit],
                photo_row,
            )?,
            None => stmt.query_map(params![text, limit], photo_row)?,
        };
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A full-text search over the file name, caption, title and keyword names, ranked by
    /// relevance. `query` follows FTS5's own syntax (a plain word is a prefix-free match; quote a
    /// phrase, or add `*` for a prefix).
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "SELECT {COLUMNS} {FROM}
             JOIN photo_fts f ON f.rowid = p.rowid
             WHERE photo_fts MATCH ?1 ORDER BY bm25(photo_fts) LIMIT ?2"
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params![query, limit], photo_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A single photo by identifier.
    pub fn photo(&self, id: &PhotoId) -> Result<Option<PhotoRow>> {
        let sql = format!("SELECT {COLUMNS} {FROM} WHERE p.id = ?1");
        Ok(self
            .conn
            .query_row(&sql, [id.to_string()], photo_row)
            .optional()?)
    }
}
