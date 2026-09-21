// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading and writing the photo sidecar (XMP), the version sidecar and the state files.
//!
//! Pure functions over bytes, no file access. Unknown content is preserved on a round trip
//! (architecture §3.1, testing strategy §3). Work package WP1.
