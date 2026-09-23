// SPDX-License-Identifier: GPL-3.0-or-later
//! What can go wrong loading or running a plugin. A plugin failing never becomes a panic in the
//! host (architecture §8.2): every wasmtime error (a trap, a denied grant, a limit reached)
//! lands here instead.

/// What went wrong loading or running a plugin.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// The bytes are not a valid WebAssembly module, or the module does not export what the host
    /// requires of every plugin (`memory`, `alloc`).
    #[error("cannot load this plugin: {0}")]
    Load(String),
    /// Creating an instance failed: a permission could not be set up, or the module's imports do
    /// not match what the host provides.
    #[error("cannot start this plugin: {0}")]
    Instantiate(String),
    /// A call into the plugin trapped, was interrupted by its time budget, or exceeded a limit.
    /// The instance that produced this error must not be reused (architecture §8.2: a failure
    /// fails that call, never the host, but the plugin's own state after a trap is undefined).
    #[error("the plugin failed: {0}")]
    Call(String),
}

/// The result type most of this crate returns.
pub type Result<T> = std::result::Result<T, HostError>;
