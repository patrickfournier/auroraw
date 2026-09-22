// SPDX-License-Identifier: GPL-3.0-or-later
//! Sources: local folders and removable volumes through the `Source` interface
//! ([`auroraw_plugin_api::source`]), states, monitoring and reconcile-against-the-catalogue
//! (architecture §9.1, design note 004). Work package WP4.
//!
//! - [`filesystem`] is the one `Source` implementation M1 needs: a plain folder, used both for a
//!   folder the photographer picked directly and for a removable volume's mount point.
//! - [`volumes`] recognises a mounted removable volume, one mechanism per platform.
//! - [`relink`] compares a fresh scan against what the catalogue already knows (design note 004
//!   §6.3-§6.4): change detection and relinking. Duplicates (several simultaneous locations,
//!   D-036) are deliberately out of scope; see [`relink`]'s own doc comment.
//! - [`fake`] is an in-memory `Source` for tests that do not want a real folder (testing strategy
//!   §3), used as a dev-dependency by other crates too.

pub mod fake;
pub mod filesystem;
pub mod relink;
pub mod volumes;
