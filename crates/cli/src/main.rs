// SPDX-License-Identifier: GPL-3.0-or-later
//! The headless command line: rebuild, verify, import, export, benchmark (architecture §3.1).
//! Work package WP3 fills it in.

use std::process::ExitCode;

fn main() -> ExitCode {
    let engine = auroraw_engine::Engine::new();
    match std::env::args().nth(1).as_deref() {
        Some("--version") => {
            println!("auroraw-cli {}", engine.version());
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: auroraw-cli --version");
            ExitCode::from(2)
        }
    }
}
