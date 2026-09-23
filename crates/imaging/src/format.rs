// SPDX-License-Identifier: GPL-3.0-or-later
//! Deciding which decoder handles a file: `image` for the standard formats it already
//! understands well (JPEG, PNG, TIFF), `rawler` for everything else (the camera RAW formats and
//! DNG, which is TIFF-based but never has this literal extension).

use std::path::Path;

/// Whether `path`'s extension names a format `image` decodes directly, rather than a camera RAW
/// format `rawler` decodes.
pub(crate) fn is_standard(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    matches!(
        ext.as_deref(),
        Some("jpg" | "jpeg" | "png" | "tif" | "tiff")
    )
}
