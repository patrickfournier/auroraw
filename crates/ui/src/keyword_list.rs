// SPDX-License-Identifier: GPL-3.0-or-later
//! The keyword panel's model (spec §5.7, D-098): the vocabulary as a flat, depth-first list of the rows that
//! are visible (a collapsed keyword hides its descendants, what was typed shows the matches with their
//! ancestors), with how many photos carry each keyword and, for the current selection, whether it carries
//! it none, some or all. Assigning is not here: the grid applies a keyword to its selection as one action.

use core::pin::Pin;
use std::collections::{HashMap, HashSet};

use auroraw_catalogue::KeywordRow;
use auroraw_engine::{Command, KeywordId, Outcome};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};

use crate::models::qobject::KeywordList;
use crate::session;

const ROLE_KEYWORD_ID: i32 = 0x0100;
const ROLE_NAME: i32 = 0x0101;
const ROLE_DEPTH: i32 = 0x0102;
const ROLE_PHOTOS: i32 = 0x0103;
const ROLE_STATE: i32 = 0x0104;
const ROLE_HAS_CHILDREN: i32 = 0x0105;
const ROLE_EXPANDED: i32 = 0x0106;

/// The Rust side of the model.
#[derive(Default)]
pub struct KeywordListRust {
    pub(crate) count: i32,
    /// The whole vocabulary, a parent before its children.
    all: Vec<KeywordRow>,
    /// The indices into `all` that are shown.
    visible: Vec<usize>,
    collapsed: HashSet<KeywordId>,
    filter: String,
    /// How many selected photos carry each keyword, and how many are selected.
    usage: HashMap<KeywordId, usize>,
    selected: usize,
}

fn text(value: &str) -> QString {
    QString::from(value)
}

impl KeywordListRust {
    fn depth(row: &KeywordRow) -> i32 {
        row.path.matches('|').count() as i32
    }

    /// Which rows of `all` are shown now.
    fn recompute(&mut self) {
        let has_parent: HashMap<KeywordId, Option<KeywordId>> =
            self.all.iter().map(|k| (k.id, k.parent)).collect();
        let needle = self.filter.to_lowercase();
        if needle.is_empty() {
            self.visible = (0..self.all.len())
                .filter(|i| {
                    // Shown unless an ancestor is collapsed.
                    let mut parent = self.all[*i].parent;
                    while let Some(p) = parent {
                        if self.collapsed.contains(&p) {
                            return false;
                        }
                        parent = has_parent.get(&p).copied().flatten();
                    }
                    true
                })
                .collect();
            return;
        }
        let mut shown: HashSet<KeywordId> = HashSet::new();
        for row in &self.all {
            if row.name.to_lowercase().contains(&needle) {
                shown.insert(row.id);
                let mut parent = row.parent;
                while let Some(p) = parent {
                    if !shown.insert(p) {
                        break;
                    }
                    parent = has_parent.get(&p).copied().flatten();
                }
            }
        }
        self.visible = (0..self.all.len())
            .filter(|i| shown.contains(&self.all[*i].id))
            .collect();
    }

    fn row(&self, row: i32) -> Option<&KeywordRow> {
        usize::try_from(row)
            .ok()
            .and_then(|r| self.visible.get(r))
            .map(|i| &self.all[*i])
    }
}

impl KeywordList {
    /// Rebuilds the visible rows and tells the views.
    fn rebuilt(mut self: Pin<&mut Self>) {
        // SAFETY: every begin is followed by its end, with nothing in between that can fail.
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().recompute();
            self.as_mut().end_reset_model();
        }
        let count = self.visible.len() as i32;
        self.set_count(count);
    }

    pub fn refresh(mut self: Pin<&mut Self>) {
        let all = session::current()
            .and_then(|s| s.engine.read_catalogue().ok())
            .and_then(|c| c.keywords_with_counts().ok())
            .unwrap_or_default();
        // The same keywords in the same places (only the counts moved): the views are told the rows
        // changed, nothing is reset, so the scroll position and the rows under the mouse stay.
        let same = all.len() == self.all.len()
            && all
                .iter()
                .zip(&self.all)
                .all(|(a, b)| a.id == b.id && a.path == b.path);
        self.as_mut().rust_mut().all = all;
        if !same {
            self.rebuilt();
            return;
        }
        let last = self.visible.len() as i32 - 1;
        if last >= 0 {
            let (first, end) = (
                self.index(0, 0, &QModelIndex::default()),
                self.index(last, 0, &QModelIndex::default()),
            );
            self.as_mut()
                .data_changed(&first, &end, &QVector::<i32>::default());
        }
    }

    pub fn set_filter(mut self: Pin<&mut Self>, filter: &QString) {
        self.as_mut().rust_mut().filter = filter.to_string().trim().to_string();
        self.rebuilt();
    }

    pub fn toggle_expanded(mut self: Pin<&mut Self>, row: i32) {
        let Some(id) = self.row(row).map(|k| k.id) else {
            return;
        };
        if self.collapsed.contains(&id) {
            self.as_mut().rust_mut().collapsed.remove(&id);
        } else {
            self.as_mut().rust_mut().collapsed.insert(id);
        }
        self.rebuilt();
    }

    pub fn apply_usage(mut self: Pin<&mut Self>, usage: &QString, selected: i32) {
        let parsed: HashMap<KeywordId, usize> =
            serde_json::from_str::<HashMap<String, usize>>(&usage.to_string())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(id, n)| id.parse().ok().map(|id| (id, n)))
                .collect();
        self.as_mut().rust_mut().usage = parsed;
        self.as_mut().rust_mut().selected = selected.max(0) as usize;
        let last = self.visible.len() as i32 - 1;
        if last >= 0 {
            let (first, end) = (
                self.index(0, 0, &QModelIndex::default()),
                self.index(last, 0, &QModelIndex::default()),
            );
            let mut roles = QVector::<i32>::default();
            roles.append(ROLE_STATE);
            self.as_mut().data_changed(&first, &end, &roles);
        }
    }

    pub fn id_at(&self, row: i32) -> QString {
        self.row(row)
            .map(|k| text(&k.id.to_string()))
            .unwrap_or_default()
    }

    pub fn name_at(&self, row: i32) -> QString {
        self.row(row).map(|k| text(&k.name)).unwrap_or_default()
    }

    pub fn best_match(&self, typed: &QString) -> i32 {
        let needle = typed.to_string().trim().to_lowercase();
        if needle.is_empty() {
            return -1;
        }
        let names: Vec<String> = self
            .visible
            .iter()
            .map(|i| self.all[*i].name.to_lowercase())
            .collect();
        names
            .iter()
            .position(|n| *n == needle)
            .or_else(|| names.iter().position(|n| n.starts_with(&needle)))
            .or_else(|| names.iter().position(|n| n.contains(&needle)))
            .map_or(-1, |row| row as i32)
    }

    pub fn create(mut self: Pin<&mut Self>, name: &QString, parent: &QString) -> QString {
        let Some(session) = session::current() else {
            return text("error:no workspace is open");
        };
        let name = name.to_string().trim().to_string();
        if name.is_empty() || name.contains('|') {
            return text("error:a keyword needs a name without |");
        }
        let parent: Option<KeywordId> = parent.to_string().parse().ok();
        // The name is there already under that parent: that keyword is the one.
        if let Some(existing) = self
            .all
            .iter()
            .find(|k| k.parent == parent && k.name.to_lowercase() == name.to_lowercase())
        {
            return text(&existing.id.to_string());
        }
        match session
            .engine
            .submit_and_wait(Command::CreateKeyword { name, parent })
        {
            Ok(Outcome::KeywordCreated(id)) => {
                self.as_mut().refresh();
                text(&id.to_string())
            }
            Ok(other) => text(&format!("error:unexpected answer {other:?}")),
            Err(e) => text(&format!("error:{e}")),
        }
    }

    pub fn rename(mut self: Pin<&mut Self>, row: i32, name: &QString) -> QString {
        let (Some(session), Some(id)) = (session::current(), self.row(row).map(|k| k.id)) else {
            return text("no such keyword");
        };
        let new_name = name.to_string().trim().to_string();
        if new_name.is_empty() || new_name.contains('|') {
            return text("a keyword needs a name without |");
        }
        match session.engine.submit_and_wait(Command::RenameKeyword {
            keyword_id: id,
            new_name,
        }) {
            Ok(_) => {
                self.as_mut().refresh();
                QString::default()
            }
            Err(e) => text(&e.to_string()),
        }
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(keyword) = self.row(index.row()) else {
            return QVariant::default();
        };
        match role {
            ROLE_KEYWORD_ID => QVariant::from(&text(&keyword.id.to_string())),
            ROLE_NAME => QVariant::from(&text(&keyword.name)),
            ROLE_DEPTH => QVariant::from(&KeywordListRust::depth(keyword)),
            ROLE_PHOTOS => QVariant::from(&(keyword.photos as i32)),
            ROLE_STATE => {
                let carried = self.usage.get(&keyword.id).copied().unwrap_or(0);
                let state = if self.selected == 0 || carried == 0 {
                    0
                } else if carried >= self.selected {
                    2
                } else {
                    1
                };
                QVariant::from(&state)
            }
            ROLE_HAS_CHILDREN => {
                QVariant::from(&self.all.iter().any(|k| k.parent == Some(keyword.id)))
            }
            ROLE_EXPANDED => QVariant::from(&(!self.collapsed.contains(&keyword.id))),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_KEYWORD_ID, QByteArray::from("keywordId"));
        roles.insert(ROLE_NAME, QByteArray::from("name"));
        roles.insert(ROLE_DEPTH, QByteArray::from("depth"));
        roles.insert(ROLE_PHOTOS, QByteArray::from("photos"));
        roles.insert(ROLE_STATE, QByteArray::from("carried"));
        roles.insert(ROLE_HAS_CHILDREN, QByteArray::from("hasChildren"));
        roles.insert(ROLE_EXPANDED, QByteArray::from("expanded"));
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.visible.len() as i32
    }
}
