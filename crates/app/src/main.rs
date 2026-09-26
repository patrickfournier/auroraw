// SPDX-License-Identifier: GPL-3.0-or-later
//! The Auroraw application: `--version` and `--self-test` for CI's launch test, and otherwise the
//! Qt Quick interface, which opens the last workspace, or the welcome list (WP8).

use std::path::PathBuf;
use std::process::ExitCode;

use auroraw_engine::LocalDirs;
use directories::{ProjectDirs, UserDirs};

/// This machine's data and cache folders (design note 001 §5.7: the catalogue database and the
/// previews are local, the workspace is what is backed up) and its Pictures folder.
fn machine_folders() -> Option<(LocalDirs, PathBuf)> {
    let project = ProjectDirs::from("org", "auroraw", "Auroraw")?;
    let user = UserDirs::new()?;
    let pictures = user
        .picture_dir()
        .map(PathBuf::from)
        .unwrap_or_else(|| user.home_dir().join("Pictures"));
    Some((
        LocalDirs {
            data: project.data_local_dir().to_path_buf(),
            cache: project.cache_dir().to_path_buf(),
        },
        pictures,
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
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
        Some(flag) if flag.starts_with("--") => usage(),
        other => {
            let Some((dirs, pictures)) = machine_folders() else {
                eprintln!("cannot find this machine's data folders");
                return ExitCode::FAILURE;
            };
            use auroraw_ui as ui;
            let launch = ui::Launch {
                dirs,
                pictures,
                open: other.map(PathBuf::from),
            };
            match ui::run(launch) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("cannot start the interface: {e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: {} [workspace folder] | --version | --self-test",
        auroraw_types::APP_NAME
    );
    ExitCode::from(2)
}
