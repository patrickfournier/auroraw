// SPDX-License-Identifier: GPL-3.0-or-later
//! Comparing what a source reports right now against what the catalogue already knows about it
//! (design note 004 §6.3-§6.4): change detection, relinking, and reporting new and missing files.
//!
//! Scoped to one source at a time, deliberately: the design note's relinking table also covers a
//! file moving *between* sources and exact duplicates becoming one photo with two locations
//! (D-036), but the catalogue does not yet index more than one location per photo (WP2's schema),
//! and duplicates are WP9's job (spec §5.3), not this one's. A found file whose match is still
//! present at its old path too is therefore reported [`ScanOutcome::Ambiguous`] rather than
//! silently linked: a person decides, and nothing is lost either way (D-019, D-031).

use std::collections::{HashMap, HashSet};

use auroraw_types::{Fingerprint, PhotoId};

/// One file a fresh scan of a source found.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundFile {
    /// The path inside the source.
    pub path: String,
    /// Its sampled fingerprint (design note 004 §6.1).
    pub fingerprint: Fingerprint,
}

/// One file the catalogue already has on record for this source.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownFile {
    /// The photo this file belongs to.
    pub photo_id: PhotoId,
    /// Where the catalogue last saw it.
    pub path: String,
    /// Its fingerprint as of the last scan.
    pub fingerprint: Fingerprint,
}

/// One thing a scan found. Nothing here is applied automatically except [`ScanOutcome::Relinked`]
/// and [`ScanOutcome::OriginalChanged`]'s catalogue-side bookkeeping (D-019: relinking is silent
/// only when the match is unique); a new file waits for confirmation, and an ambiguous or missing
/// one waits for a person. [`ScanOutcome::Confirmed`] carries no news on its own, but a caller
/// that marked a photo "original changed" or "missing" after an earlier scan needs it to know
/// when to clear that mark (a file found exactly as before, after a source was unplugged and
/// replugged, for instance): [`reconcile`] reports every known file it saw, not only the ones that
/// changed.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanOutcome {
    /// A known file is exactly as the catalogue expects: present at its known path, same
    /// fingerprint. Clears any earlier "original changed" or "missing" mark.
    Confirmed {
        /// The photo.
        photo_id: PhotoId,
    },
    /// A known file changed at its known path (design note 004 §6.5, "original changed").
    OriginalChanged {
        /// The photo.
        photo_id: PhotoId,
        /// Its path.
        path: String,
    },
    /// A known file moved or was renamed; its old location is gone and this is the only
    /// candidate. Silent (D-019): the caller is expected to apply this without asking.
    Relinked {
        /// The photo.
        photo_id: PhotoId,
        /// Where it used to be.
        from: String,
        /// Where it is now.
        to: String,
    },
    /// A found file's fingerprint matches more than one known file, or matches exactly one whose
    /// old location is still there too (a second location this work package does not model,
    /// deliberately conservative rather than merging or overwriting anything).
    Ambiguous {
        /// The found file's path.
        path: String,
        /// The known photos it might belong to.
        candidates: Vec<PhotoId>,
    },
    /// A found file that matches nothing the catalogue knows: offered for adding, never added on
    /// its own (D-019).
    New {
        /// Its path.
        path: String,
    },
    /// A known file whose path is gone and that matches no found file: kept, never removed
    /// automatically (D-019, D-031).
    Missing {
        /// The photo.
        photo_id: PhotoId,
        /// Where it used to be.
        path: String,
    },
}

/// Compares `found` (a fresh scan) against `known` (the catalogue's rows for the same source) and
/// reports what changed and what did not: every known file gets exactly one outcome (a
/// [`ScanOutcome::Confirmed`] if it is exactly as expected), so a caller can use this alone to
/// decide what to clear as well as what to set.
pub fn reconcile(found: &[FoundFile], known: &[KnownFile]) -> Vec<ScanOutcome> {
    let known_by_path: HashMap<&str, &KnownFile> =
        known.iter().map(|k| (k.path.as_str(), k)).collect();
    let found_by_path: HashMap<&str, &FoundFile> =
        found.iter().map(|f| (f.path.as_str(), f)).collect();

    let mut outcomes = Vec::new();
    let mut relinked_from: HashSet<&str> = HashSet::new();

    for f in found {
        let Some(k) = known_by_path.get(f.path.as_str()) else {
            // A path the catalogue does not know: find it by fingerprint.
            let matches: Vec<&KnownFile> = known
                .iter()
                .filter(|k| k.fingerprint == f.fingerprint)
                .collect();
            match matches.as_slice() {
                [] => outcomes.push(ScanOutcome::New {
                    path: f.path.clone(),
                }),
                [only] if found_by_path.contains_key(only.path.as_str()) => {
                    // The old location and this one both exist right now: a second location.
                    outcomes.push(ScanOutcome::Ambiguous {
                        path: f.path.clone(),
                        candidates: vec![only.photo_id],
                    });
                }
                [only] => {
                    outcomes.push(ScanOutcome::Relinked {
                        photo_id: only.photo_id,
                        from: only.path.clone(),
                        to: f.path.clone(),
                    });
                    relinked_from.insert(only.path.as_str());
                }
                several => outcomes.push(ScanOutcome::Ambiguous {
                    path: f.path.clone(),
                    candidates: several.iter().map(|k| k.photo_id).collect(),
                }),
            }
            continue;
        };
        if f.fingerprint != k.fingerprint {
            outcomes.push(ScanOutcome::OriginalChanged {
                photo_id: k.photo_id,
                path: f.path.clone(),
            });
        } else {
            outcomes.push(ScanOutcome::Confirmed {
                photo_id: k.photo_id,
            });
        }
    }

    for k in known {
        if found_by_path.contains_key(k.path.as_str()) || relinked_from.contains(k.path.as_str()) {
            continue;
        }
        outcomes.push(ScanOutcome::Missing {
            photo_id: k.photo_id,
            path: k.path.clone(),
        });
    }

    outcomes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(byte: u8) -> Fingerprint {
        Fingerprint::from_bytes([byte; 32])
    }

    fn photo(byte: u8) -> PhotoId {
        PhotoId::from_bytes([byte; 16])
    }

    #[test]
    fn an_unchanged_file_is_confirmed() {
        let known = vec![KnownFile {
            photo_id: photo(1),
            path: "a.jpg".into(),
            fingerprint: fp(1),
        }];
        let found = vec![FoundFile {
            path: "a.jpg".into(),
            fingerprint: fp(1),
        }];
        assert_eq!(
            reconcile(&found, &known),
            vec![ScanOutcome::Confirmed { photo_id: photo(1) }]
        );
    }

    #[test]
    fn a_different_fingerprint_at_the_same_path_is_original_changed() {
        let known = vec![KnownFile {
            photo_id: photo(1),
            path: "a.jpg".into(),
            fingerprint: fp(1),
        }];
        let found = vec![FoundFile {
            path: "a.jpg".into(),
            fingerprint: fp(2),
        }];
        assert_eq!(
            reconcile(&found, &known),
            vec![ScanOutcome::OriginalChanged {
                photo_id: photo(1),
                path: "a.jpg".into()
            }]
        );
    }

    #[test]
    fn a_unique_fingerprint_match_at_a_new_path_is_a_silent_relink() {
        let known = vec![KnownFile {
            photo_id: photo(1),
            path: "old/a.jpg".into(),
            fingerprint: fp(1),
        }];
        let found = vec![FoundFile {
            path: "new/a.jpg".into(),
            fingerprint: fp(1),
        }];
        assert_eq!(
            reconcile(&found, &known),
            vec![ScanOutcome::Relinked {
                photo_id: photo(1),
                from: "old/a.jpg".into(),
                to: "new/a.jpg".into()
            }]
        );
    }

    #[test]
    fn a_match_whose_old_location_is_still_there_is_ambiguous_not_a_silent_second_location() {
        let known = vec![KnownFile {
            photo_id: photo(1),
            path: "a.jpg".into(),
            fingerprint: fp(1),
        }];
        let found = vec![
            FoundFile {
                path: "a.jpg".into(),
                fingerprint: fp(1),
            },
            FoundFile {
                path: "backup/a.jpg".into(),
                fingerprint: fp(1),
            },
        ];
        assert_eq!(
            reconcile(&found, &known),
            vec![
                ScanOutcome::Confirmed { photo_id: photo(1) },
                ScanOutcome::Ambiguous {
                    path: "backup/a.jpg".into(),
                    candidates: vec![photo(1)]
                },
            ]
        );
    }

    #[test]
    fn several_known_files_sharing_a_fingerprint_make_a_new_path_ambiguous() {
        let known = vec![
            KnownFile {
                photo_id: photo(1),
                path: "a.jpg".into(),
                fingerprint: fp(1),
            },
            KnownFile {
                photo_id: photo(2),
                path: "b.jpg".into(),
                fingerprint: fp(1),
            },
        ];
        let found = vec![FoundFile {
            path: "c.jpg".into(),
            fingerprint: fp(1),
        }];
        let outcomes = reconcile(&found, &known);

        let ambiguous = outcomes
            .iter()
            .find(|o| matches!(o, ScanOutcome::Ambiguous { .. }))
            .expect("an ambiguous outcome");
        match ambiguous {
            ScanOutcome::Ambiguous { path, candidates } => {
                assert_eq!(path, "c.jpg");
                let mut c = candidates.clone();
                c.sort();
                assert_eq!(c, vec![photo(1), photo(2)]);
            }
            _ => unreachable!(),
        }
        // Ambiguity is never resolved silently: both candidates' old paths are still gone, so
        // both are also reported missing until a person picks one (or neither).
        let missing: Vec<_> = outcomes
            .iter()
            .filter_map(|o| match o {
                ScanOutcome::Missing { photo_id, .. } => Some(*photo_id),
                _ => None,
            })
            .collect();
        assert!(missing.contains(&photo(1)) && missing.contains(&photo(2)));
    }

    #[test]
    fn no_fingerprint_match_at_an_unknown_path_is_a_new_file() {
        let found = vec![FoundFile {
            path: "a.jpg".into(),
            fingerprint: fp(1),
        }];
        assert_eq!(
            reconcile(&found, &[]),
            vec![ScanOutcome::New {
                path: "a.jpg".into()
            }]
        );
    }

    #[test]
    fn a_known_file_found_nowhere_is_missing_not_removed() {
        let known = vec![KnownFile {
            photo_id: photo(1),
            path: "a.jpg".into(),
            fingerprint: fp(1),
        }];
        assert_eq!(
            reconcile(&[], &known),
            vec![ScanOutcome::Missing {
                photo_id: photo(1),
                path: "a.jpg".into()
            }]
        );
    }

    #[test]
    fn a_relinked_files_old_path_is_not_also_reported_missing() {
        let known = vec![KnownFile {
            photo_id: photo(1),
            path: "old.jpg".into(),
            fingerprint: fp(1),
        }];
        let found = vec![FoundFile {
            path: "new.jpg".into(),
            fingerprint: fp(1),
        }];
        let outcomes = reconcile(&found, &known);
        assert_eq!(outcomes.len(), 1);
        assert!(matches!(&outcomes[0], ScanOutcome::Relinked { .. }));
    }
}
