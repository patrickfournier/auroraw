// SPDX-License-Identifier: GPL-3.0-or-later
//! The WebAssembly plugin host: wasmtime, permissions, limits and the compiled cache
//! (architecture §8, decision D-076). Work package WP6.
//!
//! - [`PluginHost`] compiles plugins and starts instances of them, with the epoch-based timer
//!   every instance's time budget is measured against.
//! - [`Grants`] is what the host actually hands one instance: never more than a plugin's
//!   declared [`auroraw_plugin_api::Permissions`] asked for, and nothing unless the host (in
//!   time, the person) granted it.
//! - The host and plugin talk across a hand-made C interface, not the WebAssembly component
//!   model (architecture §8.2b): WP6 measured the component model at 11x the call cost and 3.3x
//!   the cost of copying a large buffer, for typed interfaces this crate does not need yet.
//! - [`WasmDecoder`] is [`auroraw_plugin_api::Decoder`] implemented by `plugins/rawler-decoder`,
//!   the first real plugin this loads (architecture §8.1).
//!
//! Experimental until milestone M5, like the rest of `plugin-api` (architecture §8.2b).

mod decoder;
mod error;
mod grants;
mod plugin;

pub use decoder::WasmDecoder;
pub use error::{HostError, Result};
pub use grants::Grants;
pub use plugin::{CompiledPlugin, Instance, PluginHost};
