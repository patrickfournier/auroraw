// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading and writing the photo sidecar (XMP), the version sidecar and the state files.
//!
//! Pure functions over bytes and readers, no file-system access: the workspace crate places and
//! writes the files. Unknown content is preserved on a round trip, and writing is canonical: the
//! same content always gives the same bytes (design notes 001 to 004, architecture §3.1).

pub mod fingerprint;
pub mod state;
