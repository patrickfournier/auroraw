// SPDX-License-Identifier: GPL-3.0-or-later
//! Turning paired photos into a plan: a sequence number per photo (several cameras sorted
//! together by capture time, M1 plan §6 item 6), a rendered destination for each file, and a
//! unique suffix when two files would land on the same path -- the case that same-numbered shots
//! from two different cameras produce when a template is built from the original name, which is
//! exactly the "clashing numbers" case the plan text calls out by name.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::discover::stem_and_extension;
use crate::pair::PhotoGroup;
use crate::template::{TemplateContext, render};

/// One file of a planned photo: where it is now, and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedFile {
    /// Its path inside the source.
    pub source_path: String,
    /// Its size, from the source listing.
    pub size: u64,
    /// Where it lands, relative to the destination root.
    pub destination: PathBuf,
    /// Where its verified backup copies land, each relative to its own root.
    pub backup_destinations: Vec<PathBuf>,
}

/// One photo, planned: the file that is developed, and its companion if it has one, both with
/// their destinations already decided.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedPhoto {
    /// The file that is developed.
    pub original: PlannedFile,
    /// Its companion, if the photo has one.
    pub companion: Option<PlannedFile>,
}

/// A path this run already assigned, so the next collision (two cameras' same-numbered shots
/// rendering to the same destination) gets a suffix instead of silently landing on top of it.
/// The caller may pre-seed this with paths that already exist on disk from an earlier import, so
/// a fresh run does not collide with one already there either.
pub type UsedPaths = HashSet<PathBuf>;

fn unique(mut path: PathBuf, used: &mut UsedPaths) -> PathBuf {
    if used.insert(path.clone()) {
        return path;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut n = 2u32;
    loop {
        let name = match &ext {
            Some(ext) => format!("{stem}_{n}.{ext}"),
            None => format!("{stem}_{n}"),
        };
        let candidate = parent.join(name);
        if used.insert(candidate.clone()) {
            path = candidate;
            break;
        }
        n += 1;
    }
    path
}

fn planned_file(
    source_path: &str,
    size: u64,
    ctx: &TemplateContext,
    destination_template: &str,
    backup_templates: &[String],
    used: &mut UsedPaths,
) -> PlannedFile {
    let destination = unique(PathBuf::from(render(destination_template, ctx)), used);
    let backup_destinations = backup_templates
        .iter()
        .map(|t| unique(PathBuf::from(render(t, ctx)), used))
        .collect();
    PlannedFile {
        source_path: source_path.to_string(),
        size,
        destination,
        backup_destinations,
    }
}

/// Plans every group: sorted by capture time (ties broken by source path, so the order is
/// deterministic even when two cameras agree to the second), numbered from 1, each file's
/// destination rendered from the templates and made unique against `used`.
pub fn plan(
    mut groups: Vec<PhotoGroup>,
    destination_template: &str,
    backup_templates: &[String],
    shoot: Option<&str>,
    used: &mut UsedPaths,
) -> Vec<PlannedPhoto> {
    groups.sort_by(|a, b| {
        a.original
            .capture_time
            .cmp(&b.original.capture_time)
            .then_with(|| a.original.path.cmp(&b.original.path))
    });

    let mut out = Vec::with_capacity(groups.len());
    for (i, group) in groups.into_iter().enumerate() {
        let sequence = i as u32 + 1;
        let (stem, ext) = stem_and_extension(&group.original.path);
        let ctx = TemplateContext {
            capture_time: group.original.capture_time,
            sequence,
            camera: group.original.camera.as_deref(),
            original_stem: stem,
            extension: ext,
            shoot,
        };
        let original = planned_file(
            &group.original.path,
            group.original.size,
            &ctx,
            destination_template,
            backup_templates,
            used,
        );
        let companion = group.companion.map(|c| {
            let (stem, ext) = stem_and_extension(&c.path);
            let ctx = TemplateContext {
                original_stem: stem,
                extension: ext,
                ..ctx
            };
            planned_file(
                &c.path,
                c.size,
                &ctx,
                destination_template,
                backup_templates,
                used,
            )
        });
        out.push(PlannedPhoto {
            original,
            companion,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::DiscoveredFile;
    use time::macros::datetime;

    fn file(path: &str, camera: &str, when: time::OffsetDateTime) -> DiscoveredFile {
        DiscoveredFile {
            path: path.to_string(),
            size: 10,
            capture_time: Some(when),
            camera: Some(camera.to_string()),
        }
    }

    fn group(f: DiscoveredFile) -> PhotoGroup {
        PhotoGroup {
            original: f,
            companion: None,
        }
    }

    #[test]
    fn sequence_numbers_follow_capture_time_across_cameras() {
        let a = file(
            "A/IMG_0002.CR3",
            "Camera A",
            datetime!(2026-01-01 10:00 UTC),
        );
        let b = file("B/IMG_0001.CR3", "Camera B", datetime!(2026-01-01 9:00 UTC));
        let mut used = UsedPaths::new();
        let planned = plan(
            vec![group(a), group(b)],
            "{seq:02}_{original}.{ext}",
            &[],
            None,
            &mut used,
        );
        // B was captured first, despite sorting after A alphabetically and having a lower own
        // camera-assigned number.
        assert_eq!(planned[0].original.source_path, "B/IMG_0001.CR3");
        assert_eq!(
            planned[0].original.destination,
            PathBuf::from("01_IMG_0001.cr3")
        );
        assert_eq!(planned[1].original.source_path, "A/IMG_0002.CR3");
        assert_eq!(
            planned[1].original.destination,
            PathBuf::from("02_IMG_0002.cr3")
        );
    }

    #[test]
    fn two_cameras_with_the_same_number_get_a_unique_suffix() {
        let a = file("A/IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let b = file("B/IMG_0001.CR3", "Camera B", datetime!(2026-01-01 9:05 UTC));
        let mut used = UsedPaths::new();
        let planned = plan(
            vec![group(a), group(b)],
            "{original}.{ext}",
            &[],
            None,
            &mut used,
        );
        assert_eq!(
            planned[0].original.destination,
            PathBuf::from("IMG_0001.cr3")
        );
        assert_eq!(
            planned[1].original.destination,
            PathBuf::from("IMG_0001_2.cr3"),
            "the second file to land on the same path gets a suffix, not silently overwritten"
        );
    }

    #[test]
    fn a_path_already_used_before_planning_starts_is_also_avoided() {
        let a = file("A/IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let mut used = UsedPaths::new();
        used.insert(PathBuf::from("IMG_0001.cr3"));
        let planned = plan(vec![group(a)], "{original}.{ext}", &[], None, &mut used);
        assert_eq!(
            planned[0].original.destination,
            PathBuf::from("IMG_0001_2.cr3")
        );
    }

    #[test]
    fn a_companion_renders_its_own_extension_at_the_same_sequence() {
        let mut raw = file("IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let jpeg = file("IMG_0001.JPG", "Camera A", datetime!(2026-01-01 9:00 UTC));
        raw.path = "IMG_0001.CR3".into();
        let g = PhotoGroup {
            original: raw,
            companion: Some(jpeg),
        };
        let mut used = UsedPaths::new();
        let planned = plan(vec![g], "{seq:02}_{original}.{ext}", &[], None, &mut used);
        assert_eq!(
            planned[0].companion.as_ref().unwrap().destination,
            PathBuf::from("01_IMG_0001.jpg")
        );
    }

    #[test]
    fn backup_destinations_are_rendered_and_made_unique_too() {
        let a = file("IMG_0001.CR3", "Camera A", datetime!(2026-01-01 9:00 UTC));
        let b = file("IMG_0002.CR3", "Camera A", datetime!(2026-01-01 9:01 UTC));
        let mut used = UsedPaths::new();
        let planned = plan(
            vec![group(a), group(b)],
            "{original}.{ext}",
            &["backup/{original}.{ext}".to_string()],
            None,
            &mut used,
        );
        assert_eq!(
            planned[0].original.backup_destinations,
            vec![PathBuf::from("backup/IMG_0001.cr3")]
        );
        assert_eq!(
            planned[1].original.backup_destinations,
            vec![PathBuf::from("backup/IMG_0002.cr3")]
        );
    }
}
