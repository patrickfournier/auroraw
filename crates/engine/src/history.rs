// SPDX-License-Identifier: GPL-3.0-or-later
//! The command journal (decision D-096): what a person did to their photos, so that it can be undone
//! and redone. The engine owns it because the coordinator is the single writer (architecture §4.2): it
//! sees the state before it changes it, so it records what changed, and nothing above it can miss an
//! action or record one wrongly.
//!
//! A [`Change`] is a state change as a **before and after pair**, not a command to replay: undo and redo
//! are the same operation in two directions, a never-rated photo goes back to *unset* rather than to 0,
//! and a new kind of edit adds a variant and its two directions and nothing else. An [`Entry`] is one
//! action of the person's, possibly many changes (a batch, resolving a series). The history lives in
//! memory, for the open workspace, and is bounded; the persistent history of a *version* (M2) is another
//! mechanism, in the sidecars.

use std::collections::VecDeque;

use auroraw_format::sidecar::{Flag, Metadata};
use auroraw_types::{KeywordId, PhotoId};

/// How many steps the history keeps before it forgets the oldest.
pub const DEFAULT_LIMIT: usize = 500;

/// A photo's keywords, as the sidecar holds them: identifiers and, aligned with them, their paths as of
/// the last write.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeywordSet {
    /// The identifiers.
    pub ids: Vec<KeywordId>,
    /// Their paths.
    pub paths: Vec<String>,
}

/// Which way a change is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Back to the state before.
    Undo,
    /// Forward to the state after.
    Redo,
}

/// One state change of one photo, as it was and as it became.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// The photo's own rating.
    Rating {
        /// The photo.
        photo: PhotoId,
        /// The rating before (`None`: never rated).
        before: Option<u8>,
        /// The rating after.
        after: Option<u8>,
    },
    /// The photo's own flag.
    Flag {
        /// The photo.
        photo: PhotoId,
        /// The flag before.
        before: Option<Flag>,
        /// The flag after.
        after: Option<Flag>,
    },
    /// The photo's keywords.
    Keywords {
        /// The photo.
        photo: PhotoId,
        /// The keywords before.
        before: KeywordSet,
        /// The keywords after.
        after: KeywordSet,
    },
}

impl Change {
    /// The photo the change is about.
    pub fn photo(&self) -> PhotoId {
        match self {
            Change::Rating { photo, .. }
            | Change::Flag { photo, .. }
            | Change::Keywords { photo, .. } => *photo,
        }
    }

    fn kind(&self) -> LabelKind {
        match self {
            Change::Rating { .. } => LabelKind::Rating,
            Change::Flag { .. } => LabelKind::Flag,
            Change::Keywords { .. } => LabelKind::Keywords,
        }
    }

    /// Puts `meta` in the state `direction` leads to.
    pub fn apply(&self, meta: &mut Metadata, direction: Direction) {
        let undo = direction == Direction::Undo;
        match self {
            Change::Rating { before, after, .. } => {
                meta.rating = if undo { *before } else { *after }
            }
            Change::Flag { before, after, .. } => meta.flag = if undo { *before } else { *after },
            Change::Keywords { before, after, .. } => {
                let set = if undo { before } else { after };
                meta.keyword_ids = set.ids.clone();
                meta.keyword_paths = set.paths.clone();
            }
        }
    }
}

/// What a step is about, for a menu to name it (the sentence is the interface's, so that it is translated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    /// Ratings.
    Rating,
    /// Flags.
    Flag,
    /// Keywords.
    Keywords,
    /// Several kinds of change at once.
    Batch,
}

impl LabelKind {
    /// A stable name, for the interface.
    pub fn name(self) -> &'static str {
        match self {
            LabelKind::Rating => "rating",
            LabelKind::Flag => "flag",
            LabelKind::Keywords => "keywords",
            LabelKind::Batch => "batch",
        }
    }
}

/// What a step is: a kind and how many photos it touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label {
    /// What kind of change.
    pub kind: LabelKind,
    /// How many photos.
    pub count: usize,
}

/// One action of the person's: the changes it made, in the order it made them.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// What it was.
    pub label: Label,
    /// The changes.
    pub changes: Vec<Change>,
}

impl Entry {
    /// An entry for `changes` (not empty), labelled by what they have in common.
    pub fn new(changes: Vec<Change>) -> Self {
        debug_assert!(!changes.is_empty());
        let kind = match changes.first().map(Change::kind) {
            Some(first) if changes.iter().all(|c| c.kind() == first) => first,
            _ => LabelKind::Batch,
        };
        let mut photos: Vec<PhotoId> = changes.iter().map(Change::photo).collect();
        photos.sort_by_key(|p| p.to_string());
        photos.dedup();
        Self {
            label: Label {
                kind,
                count: photos.len(),
            },
            changes,
        }
    }

    /// The photos the entry touches, each once, in the order of the changes.
    pub fn photos(&self) -> Vec<PhotoId> {
        let mut seen = Vec::new();
        for change in &self.changes {
            let photo = change.photo();
            if !seen.contains(&photo) {
                seen.push(photo);
            }
        }
        seen
    }
}

/// What a menu needs to know: what Undo and Redo would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HistoryState {
    /// What Undo would undo, if anything.
    pub undo: Option<Label>,
    /// What Redo would redo, if anything.
    pub redo: Option<Label>,
}

/// The two stacks.
#[derive(Debug)]
pub struct History {
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::with_limit(DEFAULT_LIMIT)
    }
}

impl History {
    /// A history that keeps at most `limit` steps.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Records a new action: what was undone can no longer be redone (editing after an undo discards the
    /// undone steps, spec §5.4), and the oldest step goes when there are too many.
    pub fn record(&mut self, entry: Entry) {
        self.redo.clear();
        self.undo.push_back(entry);
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
    }

    /// Takes the step Undo would undo.
    pub fn take_undo(&mut self) -> Option<Entry> {
        self.undo.pop_back()
    }

    /// Takes the step Redo would redo.
    pub fn take_redo(&mut self) -> Option<Entry> {
        self.redo.pop()
    }

    /// A step that was undone can be redone.
    pub fn put_redo(&mut self, entry: Entry) {
        self.redo.push(entry);
    }

    /// A step that was redone can be undone again (the redo stack is not cleared: it is not a new action).
    pub fn put_undo(&mut self, entry: Entry) {
        self.undo.push_back(entry);
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
    }

    /// Forgets every step that is about `photo` (it left the workspace): whether anything went.
    pub fn forget_photo(&mut self, photo: PhotoId) -> bool {
        let before = self.undo.len() + self.redo.len();
        self.undo
            .retain(|entry| entry.changes.iter().all(|c| c.photo() != photo));
        self.redo
            .retain(|entry| entry.changes.iter().all(|c| c.photo() != photo));
        self.undo.len() + self.redo.len() != before
    }

    /// What Undo and Redo would do.
    pub fn state(&self) -> HistoryState {
        HistoryState {
            undo: self.undo.back().map(|e| e.label),
            redo: self.redo.last().map(|e| e.label),
        }
    }

    /// How many steps can be undone.
    #[cfg(test)]
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rating(photo: PhotoId, before: Option<u8>, after: Option<u8>) -> Change {
        Change::Rating {
            photo,
            before,
            after,
        }
    }

    fn one(photo: PhotoId, n: u8) -> Entry {
        Entry::new(vec![rating(photo, None, Some(n))])
    }

    #[test]
    fn a_change_goes_back_and_forward_and_an_unrated_photo_goes_back_to_unset() {
        let change = rating(PhotoId::random(), None, Some(4));
        let mut meta = Metadata::default();
        change.apply(&mut meta, Direction::Redo);
        assert_eq!(meta.rating, Some(4));
        change.apply(&mut meta, Direction::Undo);
        assert_eq!(meta.rating, None, "unset, not 0");
    }

    #[test]
    fn keywords_and_their_paths_travel_together() {
        let id = KeywordId::random();
        let change = Change::Keywords {
            photo: PhotoId::random(),
            before: KeywordSet::default(),
            after: KeywordSet {
                ids: vec![id],
                paths: vec!["Travel/Peru".into()],
            },
        };
        let mut meta = Metadata::default();
        change.apply(&mut meta, Direction::Redo);
        assert_eq!((meta.keyword_ids.len(), meta.keyword_paths.len()), (1, 1));
        change.apply(&mut meta, Direction::Undo);
        assert!(meta.keyword_ids.is_empty() && meta.keyword_paths.is_empty());
    }

    #[test]
    fn a_new_action_discards_what_was_undone() {
        let (a, b) = (PhotoId::random(), PhotoId::random());
        let mut history = History::default();
        history.record(one(a, 1));
        let undone = history.take_undo().unwrap();
        history.put_redo(undone);
        assert!(history.state().redo.is_some());
        history.record(one(b, 2));
        assert_eq!(history.state().redo, None, "editing after an undo");
    }

    #[test]
    fn undo_and_redo_walk_the_stacks_in_order_without_clearing_redo() {
        let photo = PhotoId::random();
        let mut history = History::default();
        history.record(one(photo, 1));
        history.record(one(photo, 2));
        let second = history.take_undo().unwrap();
        history.put_redo(second.clone());
        let first = history.take_undo().unwrap();
        history.put_redo(first);
        assert_eq!(history.state().undo, None);
        let again = history.take_redo().unwrap();
        history.put_undo(again);
        assert!(
            history.state().redo.is_some(),
            "the second step can still be redone"
        );
        assert!(history.state().undo.is_some());
    }

    #[test]
    fn the_oldest_step_goes_when_there_are_too_many() {
        let photo = PhotoId::random();
        let mut history = History::with_limit(3);
        for n in 0..5 {
            history.record(one(photo, n));
        }
        assert_eq!(history.undo_depth(), 3);
        let newest = history.take_undo().unwrap();
        assert_eq!(newest.changes, vec![rating(photo, None, Some(4))]);
    }

    #[test]
    fn an_entry_is_labelled_by_what_its_changes_have_in_common() {
        let (a, b) = (PhotoId::random(), PhotoId::random());
        let ratings = Entry::new(vec![rating(a, None, Some(1)), rating(b, None, Some(1))]);
        assert_eq!(
            ratings.label,
            Label {
                kind: LabelKind::Rating,
                count: 2
            }
        );
        let same_photo_twice =
            Entry::new(vec![rating(a, None, Some(1)), rating(a, Some(1), Some(2))]);
        assert_eq!(same_photo_twice.label.count, 1, "photos, not changes");
        let mixed = Entry::new(vec![
            rating(a, None, Some(1)),
            Change::Flag {
                photo: b,
                before: None,
                after: Some(Flag::Picked),
            },
        ]);
        assert_eq!(mixed.label.kind, LabelKind::Batch);
        assert_eq!(mixed.photos(), vec![a, b]);
    }

    #[test]
    fn steps_about_a_photo_that_left_are_forgotten() {
        let (a, b) = (PhotoId::random(), PhotoId::random());
        let mut history = History::default();
        history.record(one(a, 1));
        history.record(one(b, 2));
        assert!(history.forget_photo(a));
        assert!(!history.forget_photo(a), "nothing left to forget");
        assert_eq!(history.undo_depth(), 1);
    }
}
