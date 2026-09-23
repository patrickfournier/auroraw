// SPDX-License-Identifier: GPL-3.0-or-later
//! RAW+JPEG pairing (D-032): "one photo with two files". Grouped by name stem, ignoring case (a
//! card's own filesystem is almost always case-insensitive).

use std::collections::HashMap;

use crate::discover::{DiscoveredFile, stem_and_extension};
use crate::profile::PairRule;

fn is_jpeg(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "jpg" | "jpeg")
}

/// One photo: the file that is developed, and, when there is one, its JPEG companion (D-032). A
/// JPEG with no RAW partner is `original` on its own, "a photo like any other" (spec §5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoGroup {
    /// The file that is developed.
    pub original: DiscoveredFile,
    /// Its companion, if a same-stem JPEG was found and paired with it.
    pub companion: Option<DiscoveredFile>,
}

/// Groups `files` into photos and applies `rule`. Only a complete pair is affected by the rule
/// ([`PairRule::RawOnly`] drops the companion; [`PairRule::JpegOnly`] keeps the companion as the
/// photo's only file instead): a RAW or a JPEG with no partner is always kept, since dropping it
/// would import nothing for that photo at all, which no rule is meant to do.
pub fn pair_files(files: Vec<DiscoveredFile>, rule: PairRule) -> Vec<PhotoGroup> {
    let mut order: Vec<String> = Vec::new();
    let mut by_stem: HashMap<String, Vec<DiscoveredFile>> = HashMap::new();
    for file in files {
        let (stem, _) = stem_and_extension(&file.path);
        let key = stem.to_ascii_lowercase();
        if !by_stem.contains_key(&key) {
            order.push(key.clone());
        }
        by_stem.entry(key).or_default().push(file);
    }

    let mut groups = Vec::new();
    for key in order {
        let members = by_stem.remove(&key).unwrap_or_default();
        let (jpegs, mut raws): (Vec<_>, Vec<_>) = members
            .into_iter()
            .partition(|f| is_jpeg(stem_and_extension(&f.path).1));

        if raws.len() == 1 && !jpegs.is_empty() {
            let original = raws.pop().unwrap();
            let mut jpegs = jpegs.into_iter();
            let companion = jpegs.next();
            groups.push(apply_rule(original, companion, rule));
            // An extra same-stem JPEG beyond the one paired above (rare) is its own photo.
            for extra in jpegs {
                groups.push(PhotoGroup {
                    original: extra,
                    companion: None,
                });
            }
        } else {
            // No RAW, several RAWs sharing a stem, or a RAW with no JPEG: nothing to pair, each
            // file becomes its own photo rather than guessing at a pairing that is not a clean
            // one-to-one match.
            for file in raws.into_iter().chain(jpegs) {
                groups.push(PhotoGroup {
                    original: file,
                    companion: None,
                });
            }
        }
    }
    groups
}

fn apply_rule(raw: DiscoveredFile, jpeg: Option<DiscoveredFile>, rule: PairRule) -> PhotoGroup {
    match (rule, jpeg) {
        (PairRule::Both, jpeg) => PhotoGroup {
            original: raw,
            companion: jpeg,
        },
        (PairRule::RawOnly, _) => PhotoGroup {
            original: raw,
            companion: None,
        },
        (PairRule::JpegOnly, Some(jpeg)) => PhotoGroup {
            original: jpeg,
            companion: None,
        },
        (PairRule::JpegOnly, None) => PhotoGroup {
            original: raw,
            companion: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> DiscoveredFile {
        DiscoveredFile {
            path: path.to_string(),
            size: 100,
            capture_time: None,
            camera: None,
        }
    }

    #[test]
    fn a_raw_and_its_jpeg_become_one_photo_with_both() {
        let groups = pair_files(
            vec![file("IMG_0001.CR3"), file("IMG_0001.JPG")],
            PairRule::Both,
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].original.path, "IMG_0001.CR3");
        assert_eq!(groups[0].companion.as_ref().unwrap().path, "IMG_0001.JPG");
    }

    #[test]
    fn pairing_is_case_insensitive_on_the_stem() {
        let groups = pair_files(
            vec![file("img_0001.CR3"), file("IMG_0001.jpg")],
            PairRule::Both,
        );
        assert_eq!(groups.len(), 1);
        assert!(groups[0].companion.is_some());
    }

    #[test]
    fn raw_only_drops_the_companion_but_keeps_the_photo() {
        let groups = pair_files(
            vec![file("IMG_0001.CR3"), file("IMG_0001.JPG")],
            PairRule::RawOnly,
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].original.path, "IMG_0001.CR3");
        assert!(groups[0].companion.is_none());
    }

    #[test]
    fn jpeg_only_keeps_the_jpeg_as_the_photos_only_file() {
        let groups = pair_files(
            vec![file("IMG_0001.CR3"), file("IMG_0001.JPG")],
            PairRule::JpegOnly,
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].original.path, "IMG_0001.JPG");
        assert!(groups[0].companion.is_none());
    }

    #[test]
    fn a_jpeg_with_no_raw_partner_is_its_own_photo_regardless_of_rule() {
        let groups = pair_files(vec![file("IMG_0002.JPG")], PairRule::RawOnly);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].original.path, "IMG_0002.JPG");
    }

    #[test]
    fn a_raw_with_no_jpeg_partner_is_its_own_photo_regardless_of_rule() {
        let groups = pair_files(vec![file("IMG_0003.CR3")], PairRule::JpegOnly);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].original.path, "IMG_0003.CR3");
    }

    #[test]
    fn unrelated_files_stay_separate_photos() {
        let groups = pair_files(
            vec![file("IMG_0001.CR3"), file("IMG_0002.CR3")],
            PairRule::Both,
        );
        assert_eq!(groups.len(), 2);
    }
}
