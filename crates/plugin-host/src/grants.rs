// SPDX-License-Identifier: GPL-3.0-or-later
//! What the host actually hands a running instance (architecture §8.2): the concrete, resolved
//! form of a plugin's declared [`auroraw_plugin_api::Permissions`], decided by the host (and, in
//! time, by what the person approved), never by the plugin itself. Nothing is granted by default.

use std::path::PathBuf;

/// What one plugin instance is allowed: folders it may read, and its memory ceiling. A time
/// budget is not part of a grant: it bounds one call, not the instance as a whole, and is passed
/// to [`crate::Instance::call`] directly. Defaults to nothing (architecture §8.2, "no access to
/// anything unless granted"): the decoder plugin WP6 ships needs none of this, matching spike 4's
/// `rawimport` ("the plugin never touches the file system").
#[derive(Debug, Clone, Default)]
pub struct Grants {
    /// Host folders the instance may read (read-only), mounted inside the sandbox at
    /// `/granted/0`, `/granted/1`, ... in this order.
    pub read_dirs: Vec<PathBuf>,
    /// The instance's memory ceiling in bytes. `None` falls back to a generous default (1 GiB)
    /// rather than no limit at all: every instance is bounded.
    pub memory_limit: Option<usize>,
}

impl Grants {
    /// The memory ceiling to actually apply: [`Self::memory_limit`], or a 1 GiB default.
    pub(crate) fn memory_limit_or_default(&self) -> usize {
        self.memory_limit.unwrap_or(1 << 30)
    }
}
