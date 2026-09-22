// SPDX-License-Identifier: GPL-3.0-or-later
//! Developer commands, run as `cargo xtask <command>` (continuous integration §4).
//!
//! - `spdx`: every source file starts with its SPDX licence identifier.
//! - `layers`: the crates depend on each other only as the architecture allows (§3.2), and a
//!   permissively licensed crate depends on nothing under the GPL (decision D-080). A
//!   dev-dependency (test-only, never linked into a shipped artifact) is unrestricted: a crate's
//!   tests may set up fixtures with any sibling crate.
//! - `check`: both of the above.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const GPL: &str = "// SPDX-License-Identifier: GPL-3.0-or-later";
const PERMISSIVE: &str = "// SPDX-License-Identifier: MIT OR Apache-2.0";

/// The crates of the workspace and the workspace crates each one may depend on **as a normal or
/// build dependency** (architecture §3.1). Dev-dependencies are not restricted by this table
/// (see `layers` below).
const ALLOWED: &[(&str, &[&str])] = &[
    ("auroraw-types", &[]),
    ("auroraw-format", &["auroraw-types"]),
    ("auroraw-catalogue", &["auroraw-format", "auroraw-types"]),
    ("auroraw-workspace", &["auroraw-format", "auroraw-types"]),
    ("auroraw-plugin-api", &[]),
    ("auroraw-plugin-host", &["auroraw-plugin-api"]),
    ("auroraw-sources", &["auroraw-plugin-api", "auroraw-types"]),
    ("auroraw-imaging", &["auroraw-plugin-api", "auroraw-types"]),
    (
        "auroraw-import",
        &["auroraw-imaging", "auroraw-sources", "auroraw-types"],
    ),
    (
        "auroraw-engine",
        &[
            "auroraw-catalogue",
            "auroraw-workspace",
            "auroraw-format",
            "auroraw-sources",
            "auroraw-import",
            "auroraw-imaging",
            "auroraw-plugin-host",
            "auroraw-types",
        ],
    ),
    ("auroraw-ui", &["auroraw-engine"]),
    (
        "auroraw-cli",
        &[
            "auroraw-engine",
            "auroraw-catalogue",
            "auroraw-format",
            "auroraw-types",
        ],
    ),
    (
        "auroraw-app",
        &["auroraw-ui", "auroraw-engine", "auroraw-types"],
    ),
    ("auroraw-testkit", &[]),
    ("xtask", &[]),
];

fn main() -> ExitCode {
    let ok = match std::env::args().nth(1).as_deref() {
        Some("spdx") => spdx(),
        Some("layers") => layers(),
        Some("check") => {
            let a = spdx();
            let b = layers();
            a && b
        }
        _ => {
            eprintln!("usage: cargo xtask <spdx|layers|check>");
            return ExitCode::from(2);
        }
    };
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root")
        .to_path_buf()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn spdx() -> bool {
    let root = root();
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("xtask"), &mut files);
    files.sort();
    let mut ok = true;
    for file in &files {
        let permissive = file.starts_with(root.join("crates").join("plugin-api"));
        let expected = if permissive { PERMISSIVE } else { GPL };
        let text = std::fs::read_to_string(file).unwrap_or_default();
        if text.lines().next() != Some(expected) {
            eprintln!(
                "{}: the first line must be `{expected}`",
                file.strip_prefix(&root).unwrap_or(file).display()
            );
            ok = false;
        }
    }
    println!(
        "spdx: {} files checked{}",
        files.len(),
        if ok { "" } else { ", with errors" }
    );
    ok
}

fn layers() -> bool {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root())
        .output()
        .expect("cargo metadata");
    if !out.status.success() {
        eprintln!("cargo metadata failed");
        return false;
    }
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("metadata is JSON");
    let packages = meta["packages"].as_array().expect("packages");
    let names: Vec<&str> = packages.iter().filter_map(|p| p["name"].as_str()).collect();
    let mut ok = true;
    for name in &names {
        if !ALLOWED.iter().any(|(n, _)| n == name) {
            eprintln!(
                "{name}: not in the table of allowed dependencies (xtask/src/main.rs); add it"
            );
            ok = false;
        }
    }
    for package in packages {
        let name = package["name"].as_str().unwrap_or_default();
        let license = package["license"].as_str().unwrap_or_default();
        let allowed = ALLOWED
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, a)| *a)
            .unwrap_or(&[]);
        for dep in package["dependencies"].as_array().into_iter().flatten() {
            let dep_name = dep["name"].as_str().unwrap_or_default();
            if !names.contains(&dep_name) {
                continue; // an external crate: `cargo deny` checks those
            }
            // A dev-dependency is test-only: cargo never links it into a shipped library or
            // binary, so it cannot leak a GPL crate into a permissive one or blur the runtime
            // layering. Tests may freely set up fixtures with any sibling crate.
            let dev = dep["kind"].as_str() == Some("dev");
            if !dev && !allowed.contains(&dep_name) {
                eprintln!("{name} must not depend on {dep_name} (architecture §3.2)");
                ok = false;
            }
            let dep_license = packages
                .iter()
                .find(|p| p["name"].as_str() == Some(dep_name))
                .and_then(|p| p["license"].as_str())
                .unwrap_or_default();
            if license.contains("MIT") && !dev && !dep_license.contains("MIT") {
                eprintln!(
                    "{name} is permissively licensed but depends on {dep_name} ({dep_license}) (D-080)"
                );
                ok = false;
            }
        }
    }
    println!(
        "layers: {} crates checked{}",
        names.len(),
        if ok { "" } else { ", with errors" }
    );
    ok
}
