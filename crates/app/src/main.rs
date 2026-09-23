// SPDX-License-Identifier: GPL-3.0-or-later
//! The Auroraw application: `--version` and `--self-test` for CI's launch test, and, given a
//! workspace folder, the real Slint shell (WP8).

use std::path::Path;
use std::process::ExitCode;

/// Opens `workspace_path` (creating it if this is the first launch there) and its catalogue,
/// alongside it, following the workspace's own `.auroraw/` convention for what is local to this
/// machine rather than synced (design note 001 §5.4): the catalogue and the previews database
/// (D-075) both live there. This crate resolves no platform cache directory yet (every crate
/// under `engine` defers that to here, and here defers it further, honestly: a real cache
/// directory, XDG/AppData/Library, is still open work, not done by WP8).
fn open_or_create(
    workspace_path: &Path,
) -> auroraw_engine::Result<(
    auroraw_engine::Engine,
    auroraw_engine::EventReceiver,
    std::path::PathBuf,
)> {
    let local = workspace_path.join(".auroraw");
    let catalogue_path = local.join("catalogue.sqlite");
    let previews_path = local.join("previews.db");
    let name = workspace_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Library".into());
    let (engine, events) = if catalogue_path.exists() {
        auroraw_engine::Engine::open(workspace_path, &catalogue_path)?
    } else {
        auroraw_engine::Engine::create(workspace_path, &catalogue_path, &name)?
    };
    Ok((engine, events, previews_path))
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
        Some(path) if !path.starts_with("--") => {
            let (engine, events, previews_path) = match open_or_create(Path::new(path)) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("cannot open {path}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            match auroraw_ui::run(engine, events, &previews_path) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("cannot start the interface: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!(
                "usage: {} <workspace folder> | --version | --self-test",
                auroraw_types::APP_NAME
            );
            ExitCode::from(2)
        }
    }
}
