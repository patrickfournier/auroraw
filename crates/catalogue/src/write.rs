// SPDX-License-Identifier: GPL-3.0-or-later
//! Incremental writes: updating one photo's or one keyword's row without a full rebuild
//! (M1 plan WP2's "batch changes in one transaction", completed here as WP3's engine needs it).
//!
//! Each function here touches only the columns its caller's kind of change can affect: a rating
//! or keyword edit never rewrites `capture_time`, `series_id` or `version_count`, for instance.
//! That is what lets these run as a small, fast update instead of the delete-and-reinsert
//! [`crate::rebuild_to_file`] does for a whole catalogue.

use auroraw_format::sidecar::{PhotoSidecar, VersionSidecar};
use auroraw_format::state::KeywordEntry;
use auroraw_types::PhotoId;
use rusqlite::{OptionalExtension, params};

use crate::SidecarStat;
use crate::effective::{effective_flag, effective_rating, flag_code};
use crate::error::{CatalogueError, Result};
use crate::open::Catalogue;

impl Catalogue {
    /// Applies a photo's own metadata (rating, flag, label, title, caption, keywords) to its
    /// existing row. `main_version`, if the photo has one, is used to compute the effective
    /// rating and flag (D-063); everything else about the photo (its files, camera, capture time,
    /// series membership, version count) is left exactly as it was. Fails if the photo is not
    /// already in the catalogue: a fresh photo enters through a rebuild, WP4's import, or (later)
    /// an explicit `add_photo`, which this crate does not need yet.
    pub fn apply_photo_metadata(
        &mut self,
        photo: &PhotoSidecar,
        stat: SidecarStat,
        main_version: Option<&VersionSidecar>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let id = photo.photo_id.to_string();
        let own_rating = photo.meta.rating.unwrap_or(0) as i64;
        let own_flag = flag_code(photo.meta.flag);
        let (eff_rating, overridden) = effective_rating(photo, main_version);
        let eff_flag = effective_flag(photo, main_version);

        let changed = tx.execute(
            "UPDATE photo SET rating = ?1, flag = ?2, label = ?3, title = ?4, caption = ?5,
                effective_rating = ?6, effective_flag = ?7, rating_overridden = ?8,
                sidecar_size = ?9, sidecar_modified = ?10
             WHERE id = ?11",
            params![
                own_rating,
                own_flag,
                photo.meta.label,
                photo.meta.title,
                photo.meta.caption,
                eff_rating,
                eff_flag,
                overridden as i64,
                stat.size as i64,
                stat.modified,
                id
            ],
        )?;
        if changed == 0 {
            return Err(CatalogueError::NotFound { kind: "photo", id });
        }

        tx.execute("DELETE FROM photo_keyword WHERE photo_id = ?1", [&id])?;
        for keyword_id in &photo.meta.keyword_ids {
            tx.execute(
                "INSERT OR IGNORE INTO photo_keyword(photo_id, keyword_id) VALUES (?1, ?2)",
                params![id, keyword_id.to_string()],
            )?;
        }
        let keyword_names = photo
            .meta
            .keyword_paths
            .iter()
            .map(|p| p.rsplit('|').next().unwrap_or(p))
            .collect::<Vec<_>>()
            .join(" ");
        tx.execute(
            "UPDATE photo_fts SET caption = ?1, title = ?2, keywords = ?3
             WHERE rowid IN (SELECT rowid FROM photo WHERE id = ?4)",
            params![photo.meta.caption, photo.meta.title, keyword_names, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The row an existing photo has, `None` if it is not in the catalogue: lets a caller decide
    /// whether it needs the main version's sidecar before calling [`Self::apply_photo_metadata`]
    /// (only when the photo actually has one).
    pub fn main_version_of(&self, id: &PhotoId) -> Result<Option<auroraw_types::VersionId>> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT main_version_id FROM photo WHERE id = ?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(text.and_then(|t| t.parse().ok()))
    }

    /// Inserts or updates one keyword's `name` and `path`. The path is the caller's
    /// responsibility (`crate::keyword_paths`): a rename can change more than this one row, if
    /// the keyword has descendants, and the caller updates each affected row the same way.
    pub fn apply_keyword(&mut self, entry: &KeywordEntry, path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO keyword(id, parent_id, name, path, export) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                parent_id = excluded.parent_id, name = excluded.name, path = excluded.path, export = excluded.export",
            params![entry.id.to_string(), entry.parent.map(|p| p.to_string()), entry.name, path, entry.export as i64],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rebuild::{RebuildInput, build};
    use auroraw_format::sidecar::Flag;
    use auroraw_types::{KeywordId, WorkspaceId};

    fn one_photo() -> (PhotoSidecar, SidecarStat) {
        let mut p = PhotoSidecar::new(PhotoId::random());
        p.meta.rating = Some(2);
        p.meta.keyword_ids = vec![KeywordId::from_bytes([1; 8])];
        p.meta.keyword_paths = vec!["Fauna|Heron".into()];
        (
            p,
            SidecarStat {
                size: 100,
                modified: Some(1),
            },
        )
    }

    /// Both keywords `one_photo` and the tests' edits can reference, so `photo_keyword`'s foreign
    /// key is satisfied.
    fn vocabulary() -> Vec<KeywordEntry> {
        [([1u8; 8], "Heron"), ([2; 8], "Lake")]
            .into_iter()
            .map(|(id, name)| KeywordEntry {
                id: KeywordId::from_bytes(id),
                name: name.into(),
                parent: None,
                synonyms: vec![],
                export: true,
                extra: Default::default(),
            })
            .collect()
    }

    #[test]
    fn apply_photo_metadata_changes_only_what_it_should() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (photo, stat) = one_photo();
        let vocab = vocabulary();
        build(
            &mut cat,
            &RebuildInput {
                photos: &[(photo.clone(), stat)],
                vocabulary: &vocab,
                ..Default::default()
            },
        )
        .unwrap();
        let before = cat.photo(&photo.photo_id).unwrap().unwrap();

        let mut edited = photo.clone();
        edited.meta.rating = Some(5);
        edited.meta.flag = Some(Flag::Picked);
        let new_stat = SidecarStat {
            size: 150,
            modified: Some(2),
        };
        cat.apply_photo_metadata(&edited, new_stat, None).unwrap();

        let after = cat.photo(&photo.photo_id).unwrap().unwrap();
        assert_eq!(after.rating, 5);
        assert_eq!(after.effective_rating, 5);
        assert_eq!(after.flag, 1);
        assert_eq!(
            after.filename, before.filename,
            "untouched by a rating change"
        );
        assert_eq!(after.camera, before.camera, "untouched by a rating change");
    }

    #[test]
    fn apply_photo_metadata_updates_keywords_and_the_search_index() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (photo, stat) = one_photo();
        let vocab = vocabulary();
        build(
            &mut cat,
            &RebuildInput {
                photos: &[(photo.clone(), stat)],
                vocabulary: &vocab,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            cat.list_by_keyword(&KeywordId::from_bytes([1; 8]), false, None, 10)
                .unwrap()
                .len(),
            1
        );

        let mut edited = photo.clone();
        edited.meta.keyword_ids = vec![KeywordId::from_bytes([2; 8])];
        edited.meta.keyword_paths = vec!["Places|Lake".into()];
        edited.meta.title = Some("A heron at the lake".into());
        cat.apply_photo_metadata(&edited, SidecarStat::default(), None)
            .unwrap();

        assert!(
            cat.list_by_keyword(&KeywordId::from_bytes([1; 8]), false, None, 10)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            cat.list_by_keyword(&KeywordId::from_bytes([2; 8]), false, None, 10)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(cat.search("heron", 10).unwrap().len(), 1);
    }

    #[test]
    fn apply_photo_metadata_on_an_unknown_photo_is_an_error() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (photo, stat) = one_photo();
        assert!(matches!(
            cat.apply_photo_metadata(&photo, stat, None),
            Err(CatalogueError::NotFound { kind: "photo", .. })
        ));
    }

    #[test]
    fn apply_keyword_inserts_then_updates() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let entry = KeywordEntry {
            id: KeywordId::from_bytes([9; 8]),
            name: "Fauna".into(),
            parent: None,
            synonyms: vec![],
            export: true,
            extra: Default::default(),
        };
        cat.apply_keyword(&entry, "Fauna").unwrap();
        cat.apply_keyword(
            &KeywordEntry {
                name: "Wildlife".into(),
                ..entry.clone()
            },
            "Wildlife",
        )
        .unwrap();
        let name: String = cat
            .conn
            .query_row(
                "SELECT name FROM keyword WHERE id = ?1",
                [entry.id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Wildlife");
    }
}
