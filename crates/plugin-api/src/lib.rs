// SPDX-License-Identifier: MIT OR Apache-2.0
//! The plugin API: the declaration schema and the interfaces between Auroraw and its plugins.
//!
//! This crate is licensed MIT OR Apache-2.0 so that a plugin, free or not, can include it
//! (decision D-080). It therefore depends on no crate of the application. The declaration schema
//! and the wasmtime host are WP6; the [`source`] interface arrives first (WP4), since the core's
//! own local-folder and removable-volume sources need it before any of that exists (architecture
//! §8.6) and run as native code, not sandboxed plugins, until WP6 wraps them. Experimental until
//! milestone M5.

pub mod source;
