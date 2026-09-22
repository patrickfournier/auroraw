// SPDX-License-Identifier: GPL-3.0-or-later
//! The Auroraw application. The interface arrives with work package WP8; for now the binary can
//! report its version and run a self-test, which is what the launch test of CI uses.

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => {
            println!(
                "{} {}",
                auroraw_types::APP_NAME,
                auroraw_engine::Engine::version()
            );
            ExitCode::SUCCESS
        }
        Some("--self-test") => {
            // Grows with the work packages: open a sample workspace, render a sample image, exit.
            println!(
                "self-test ok: {} {}",
                auroraw_types::APP_NAME,
                auroraw_engine::Engine::version()
            );
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("The interface is not built yet. Try --version or --self-test.");
            ExitCode::from(2)
        }
    }
}
