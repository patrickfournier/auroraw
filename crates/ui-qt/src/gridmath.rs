// SPDX-License-Identifier: GPL-3.0-or-later
//! Where a keyboard move in the grid goes, beyond a step to a neighbour (the arrows are the
//! `GridView`'s own). Pure, so it is unit-tested without Qt. The same rules as the Slint shell's
//! `grid::jump`.

/// The index a jump lands on: a page moves by the rows that fit on screen (at least one), keeps its
/// column, and stops at the first or last photo (the last row may be shorter than the column, so a
/// page down onto it lands on its last photo). `kind` is `page-up`, `page-down`, `home` or `end`; an
/// unknown kind leaves `current` where it is.
pub fn jump(kind: &str, current: usize, cols: usize, visible_rows: usize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let page = visible_rows.max(1) * cols.max(1);
    match kind {
        "home" => 0,
        "end" => len - 1,
        "page-up" => current.saturating_sub(page),
        "page-down" => (current + page).min(len - 1),
        _ => current.min(len - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_end_and_pages_land_on_photos_and_keep_their_column() {
        // 100 photos, 8 per row, 5 rows on screen: a page is 40 photos.
        assert_eq!(jump("home", 57, 8, 5, 100), 0);
        assert_eq!(jump("end", 3, 8, 5, 100), 99);
        assert_eq!(jump("page-down", 3, 8, 5, 100), 43);
        assert_eq!(
            jump("page-down", 70, 8, 5, 100),
            99,
            "stops at the last photo"
        );
        assert_eq!(jump("page-up", 43, 8, 5, 100), 3);
        assert_eq!(
            jump("page-up", 10, 8, 5, 100),
            0,
            "stops at the first photo"
        );
        assert_eq!(
            jump("page-down", 0, 8, 0, 100),
            8,
            "a page is a row at least"
        );
        assert_eq!(jump("end", 0, 8, 5, 0), 0, "an empty list");
        assert_eq!(
            jump("sideways", 5, 8, 5, 100),
            5,
            "an unknown kind stays put"
        );
    }
}
