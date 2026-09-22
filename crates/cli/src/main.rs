// SPDX-License-Identifier: GPL-3.0-or-later
//! The headless command line: script the engine with no window (architecture §3.1). WP3 wires up
//! the commands that do not need sources or develop yet: `create`, `list`, `rebuild`, `verify`,
//! `rate`, `flag` and `keyword`. `add-folder` (a source) and `export` follow in later work
//! packages.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use auroraw_catalogue::CatalogueError;
use auroraw_engine::{Command, Engine, EngineError, Event, EventReceiver, JobId, Outcome};
use auroraw_format::sidecar::Flag;
use auroraw_types::{KeywordId, PhotoId};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage) => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Engine(e)) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "\
usage: auroraw-cli --version
       auroraw-cli create <workspace-dir> <catalogue-file> <name>
       auroraw-cli list <workspace-dir> <catalogue-file> [--min-rating N]
       auroraw-cli rebuild <workspace-dir> <catalogue-file>
       auroraw-cli verify <workspace-dir> <catalogue-file>
       auroraw-cli rate <workspace-dir> <catalogue-file> <photo-id> <0-5>
       auroraw-cli flag <workspace-dir> <catalogue-file> <photo-id> <pick|reject|none>
       auroraw-cli keyword create <workspace-dir> <catalogue-file> <name> [--parent <id>]
       auroraw-cli keyword rename <workspace-dir> <catalogue-file> <keyword-id> <new-name>
       auroraw-cli keyword add <workspace-dir> <catalogue-file> <photo-id> <keyword-id>
       auroraw-cli keyword remove <workspace-dir> <catalogue-file> <photo-id> <keyword-id>";

enum CliError {
    Usage,
    Engine(EngineError),
}

impl From<EngineError> for CliError {
    fn from(e: EngineError) -> Self {
        CliError::Engine(e)
    }
}

impl From<CatalogueError> for CliError {
    fn from(e: CatalogueError) -> Self {
        CliError::Engine(e.into())
    }
}

fn run(args: &[String]) -> Result<(), CliError> {
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("auroraw-cli {}", Engine::version());
            Ok(())
        }
        Some("create") => {
            let (workspace, catalogue, name) = paths3(&args[1..])?;
            let (_engine, _events) = Engine::create(&workspace, &catalogue, &name)?;
            println!(
                "created {} indexed by {}",
                workspace.display(),
                catalogue.display()
            );
            Ok(())
        }
        Some("list") => cmd_list(&args[1..]),
        Some("rebuild") => cmd_rebuild(&args[1..]),
        Some("verify") => cmd_verify(&args[1..]),
        Some("rate") => cmd_rate(&args[1..]),
        Some("flag") => cmd_flag(&args[1..]),
        Some("keyword") => cmd_keyword(&args[1..]),
        _ => Err(CliError::Usage),
    }
}

fn paths2(args: &[String]) -> Result<(PathBuf, PathBuf), CliError> {
    match args {
        [workspace, catalogue] => Ok((PathBuf::from(workspace), PathBuf::from(catalogue))),
        _ => Err(CliError::Usage),
    }
}

fn paths3(args: &[String]) -> Result<(PathBuf, PathBuf, String), CliError> {
    match args {
        [workspace, catalogue, name] => Ok((
            PathBuf::from(workspace),
            PathBuf::from(catalogue),
            name.clone(),
        )),
        _ => Err(CliError::Usage),
    }
}

fn open(workspace: &Path, catalogue: &Path) -> Result<(Engine, EventReceiver), CliError> {
    Ok(Engine::open(workspace, catalogue)?)
}

/// Blocks until `job` reports finished or cancelled, printing progress as it goes.
fn wait_for_job(events: &EventReceiver, job: JobId) {
    loop {
        match events.recv_timeout(Duration::from_secs(30)) {
            Some(Event::JobProgress {
                job: j,
                done,
                total,
            }) if j == job => {
                println!("  refreshing... {done}/{total}");
            }
            Some(Event::JobFinished(j)) if j == job => {
                println!("  refresh finished");
                return;
            }
            Some(Event::JobCancelled(j)) if j == job => {
                println!("  refresh cancelled");
                return;
            }
            Some(_) => continue,
            None => {
                eprintln!("  warning: no news from job {job} in 30s, giving up waiting");
                return;
            }
        }
    }
}

fn cmd_rebuild(args: &[String]) -> Result<(), CliError> {
    let (workspace, catalogue) = paths2(args)?;
    let (engine, _events) = open(&workspace, &catalogue)?;
    match engine.submit_and_wait(Command::Rebuild)? {
        Outcome::Applied => println!("rebuilt"),
        other => unreachable!("Rebuild always returns Applied, got {other:?}"),
    }
    Ok(())
}

fn cmd_verify(args: &[String]) -> Result<(), CliError> {
    let (workspace, catalogue) = paths2(args)?;
    let (engine, _events) = open(&workspace, &catalogue)?;
    match engine.submit_and_wait(Command::Reconcile)? {
        Outcome::Applied => println!("verified"),
        other => unreachable!("Reconcile always returns Applied, got {other:?}"),
    }
    Ok(())
}

fn cmd_list(args: &[String]) -> Result<(), CliError> {
    let (workspace, catalogue, min_rating) = match args {
        [workspace, catalogue] => (workspace, catalogue, None),
        [workspace, catalogue, flag, value] if flag == "--min-rating" => (
            workspace,
            catalogue,
            Some(value.parse::<u8>().map_err(|_| CliError::Usage)?),
        ),
        _ => return Err(CliError::Usage),
    };
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    let catalogue = engine.read_catalogue()?;
    let rows = match min_rating {
        Some(min) => catalogue.list_by_min_rating(min, None, 10_000)?,
        None => catalogue.list_recent(None, 10_000)?,
    };
    for row in &rows {
        let flag = match row.flag {
            1 => "picked",
            2 => "rejected",
            _ => "-",
        };
        println!(
            "{}  rating={}  flag={}  {}",
            row.id,
            row.effective_rating,
            flag,
            row.title.as_deref().unwrap_or(&row.filename)
        );
    }
    println!("{} photo(s)", rows.len());
    Ok(())
}

fn cmd_rate(args: &[String]) -> Result<(), CliError> {
    let [workspace, catalogue, photo_id, rating] = args else {
        return Err(CliError::Usage);
    };
    let photo_id: PhotoId = photo_id.parse().map_err(|_| CliError::Usage)?;
    let rating: u8 = rating.parse().map_err(|_| CliError::Usage)?;
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    engine.submit_and_wait(Command::SetRating { photo_id, rating })?;
    println!("rated {photo_id} {rating}");
    Ok(())
}

fn cmd_flag(args: &[String]) -> Result<(), CliError> {
    let [workspace, catalogue, photo_id, flag] = args else {
        return Err(CliError::Usage);
    };
    let photo_id: PhotoId = photo_id.parse().map_err(|_| CliError::Usage)?;
    let flag = match flag.as_str() {
        "pick" => Some(Flag::Picked),
        "reject" => Some(Flag::Rejected),
        "none" => None,
        _ => return Err(CliError::Usage),
    };
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    engine.submit_and_wait(Command::SetFlag { photo_id, flag })?;
    println!(
        "flagged {photo_id} {}",
        flag.map(|f| format!("{f:?}"))
            .unwrap_or_else(|| "none".into())
    );
    Ok(())
}

fn cmd_keyword(args: &[String]) -> Result<(), CliError> {
    match args.first().map(String::as_str) {
        Some("create") => cmd_keyword_create(&args[1..]),
        Some("rename") => cmd_keyword_rename(&args[1..]),
        Some("add") => cmd_keyword_edit(&args[1..], true),
        Some("remove") => cmd_keyword_edit(&args[1..], false),
        _ => Err(CliError::Usage),
    }
}

fn cmd_keyword_create(args: &[String]) -> Result<(), CliError> {
    let (parent, rest) = take_parent(args)?;
    let [workspace, catalogue, name] = rest else {
        return Err(CliError::Usage);
    };
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    let Outcome::KeywordCreated(id) = engine.submit_and_wait(Command::CreateKeyword {
        name: name.clone(),
        parent,
    })?
    else {
        unreachable!("CreateKeyword always returns KeywordCreated");
    };
    println!("created keyword {id}");
    Ok(())
}

fn take_parent(args: &[String]) -> Result<(Option<KeywordId>, &[String]), CliError> {
    if args.first().map(String::as_str) == Some("--parent") {
        let value = args.get(1).ok_or(CliError::Usage)?;
        let id: KeywordId = value.parse().map_err(|_| CliError::Usage)?;
        Ok((Some(id), &args[2..]))
    } else {
        Ok((None, args))
    }
}

fn cmd_keyword_rename(args: &[String]) -> Result<(), CliError> {
    let [workspace, catalogue, keyword_id, new_name] = args else {
        return Err(CliError::Usage);
    };
    let keyword_id: KeywordId = keyword_id.parse().map_err(|_| CliError::Usage)?;
    let (engine, events) = open(Path::new(workspace), Path::new(catalogue))?;
    let Outcome::RenameStarted { job, affected } =
        engine.submit_and_wait(Command::RenameKeyword {
            keyword_id,
            new_name: new_name.clone(),
        })?
    else {
        unreachable!("RenameKeyword always returns RenameStarted");
    };
    println!("renamed {keyword_id}; refreshing {affected} sidecar(s)");
    wait_for_job(&events, job);
    Ok(())
}

fn cmd_keyword_edit(args: &[String], add: bool) -> Result<(), CliError> {
    let [workspace, catalogue, photo_id, keyword_id] = args else {
        return Err(CliError::Usage);
    };
    let photo_id: PhotoId = photo_id.parse().map_err(|_| CliError::Usage)?;
    let keyword_id: KeywordId = keyword_id.parse().map_err(|_| CliError::Usage)?;
    let (engine, _events) = open(Path::new(workspace), Path::new(catalogue))?;
    let command = if add {
        Command::AddKeyword {
            photo_id,
            keyword_id,
        }
    } else {
        Command::RemoveKeyword {
            photo_id,
            keyword_id,
        }
    };
    engine.submit_and_wait(command)?;
    println!(
        "{} keyword {keyword_id} {} photo {photo_id}",
        if add { "added" } else { "removed" },
        if add { "to" } else { "from" }
    );
    Ok(())
}
