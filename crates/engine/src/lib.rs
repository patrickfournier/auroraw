// SPDX-License-Identifier: GPL-3.0-or-later
//! The application core: owns the catalogues, workspaces, jobs and events, and is the only thing
//! the interface talks to (architecture §3.2, §4). It runs without a window. Work package WP3.

/// The engine. For now it holds nothing: the pieces arrive with the work packages.
#[derive(Debug, Default)]
pub struct Engine {}

impl Engine {
    /// A new engine.
    pub fn new() -> Self {
        Self {}
    }

    /// The application version.
    pub fn version(&self) -> &'static str {
        auroraw_types::app_version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_reports_the_application_version() {
        assert_eq!(Engine::new().version(), auroraw_types::app_version());
    }
}
