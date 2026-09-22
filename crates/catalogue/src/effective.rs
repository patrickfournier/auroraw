// SPDX-License-Identifier: GPL-3.0-or-later
//! The effective rating and flag of a photo (D-063): the main version's value if it overrides
//! one, otherwise the photo's own. Cull mode works on the photo's values, Develop on the
//! version's; the grid shows the effective value and marks a photo whose value is overridden.

use auroraw_format::sidecar::{Flag, OverrideField, PhotoSidecar, VersionSidecar};

/// The effective rating (0 to 5) and whether it comes from the main version's override.
pub fn effective_rating(photo: &PhotoSidecar, main_version: Option<&VersionSidecar>) -> (u8, bool) {
    match main_version {
        Some(v) if v.overrides.contains(&OverrideField::Rating) => {
            (v.meta.rating.unwrap_or(0), true)
        }
        _ => (photo.meta.rating.unwrap_or(0), false),
    }
}

/// The effective flag, as the small integer the catalogue stores (0 none, 1 picked, 2 rejected).
pub fn effective_flag(photo: &PhotoSidecar, main_version: Option<&VersionSidecar>) -> u8 {
    let flag = match main_version {
        Some(v) if v.overrides.contains(&OverrideField::Flag) => v.meta.flag,
        _ => photo.meta.flag,
    };
    flag_code(flag)
}

/// The small integer a flag is stored as.
pub(crate) fn flag_code(flag: Option<Flag>) -> u8 {
    match flag {
        None => 0,
        Some(Flag::Picked) => 1,
        Some(Flag::Rejected) => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auroraw_types::{PhotoId, VersionId};

    fn photo_with_rating(rating: u8) -> PhotoSidecar {
        let mut p = PhotoSidecar::new(PhotoId::random());
        p.meta.rating = Some(rating);
        p
    }

    #[test]
    fn without_a_main_version_the_photos_own_value_is_effective() {
        let p = photo_with_rating(3);
        assert_eq!(effective_rating(&p, None), (3, false));
        assert_eq!(effective_flag(&p, None), 0);
    }

    #[test]
    fn a_main_version_without_an_override_still_defers_to_the_photo() {
        let p = photo_with_rating(3);
        let v = VersionSidecar::new(&p, VersionId::random());
        assert_eq!(effective_rating(&p, Some(&v)), (3, false));
    }

    #[test]
    fn an_overriding_main_version_wins_and_is_marked() {
        let p = photo_with_rating(3);
        let mut v = VersionSidecar::new(&p, VersionId::random());
        v.overrides = vec![OverrideField::Rating];
        v.meta.rating = Some(5);
        assert_eq!(effective_rating(&p, Some(&v)), (5, true));

        let mut v2 = VersionSidecar::new(&p, VersionId::random());
        v2.overrides = vec![OverrideField::Flag];
        v2.meta.flag = Some(Flag::Rejected);
        assert_eq!(effective_flag(&p, Some(&v2)), 2);
        assert_eq!(
            effective_rating(&p, Some(&v2)),
            (3, false),
            "rating is not overridden by v2"
        );
    }
}
