// SPDX-License-Identifier: GPL-3.0-or-later
//! The launch test: the application binary starts, runs its self-test and exits.

use std::process::Command;

#[test]
fn self_test_passes() {
    let out = Command::new(env!("CARGO_BIN_EXE_auroraw"))
        .arg("--self-test")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("self-test ok")
    );
}
