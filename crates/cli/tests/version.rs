// SPDX-License-Identifier: GPL-3.0-or-later
//! The command line starts and reports its version, on every platform.

use std::process::Command;

#[test]
fn prints_the_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_auroraw-cli"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("auroraw-cli "), "{text:?}");
}

#[test]
fn refuses_unknown_arguments() {
    let out = Command::new(env!("CARGO_BIN_EXE_auroraw-cli"))
        .arg("--nonsense")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
