// SPDX-License-Identifier: GPL-3.0-or-later
//! Lets the application find the Qt libraries it links at run time from where the build found them: the
//! interface crate's own link arguments only reach its own binaries, and a Qt installed outside the
//! system's library folders (the online installer, `aqtinstall`; frameworks on macOS) is otherwise not
//! found when the application starts. Windows finds its DLLs through the `PATH`, and a packaged
//! application carries its own libraries (the release work package).

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=QMAKE");
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target != "linux" && target != "macos" {
        return;
    }
    let qmake = std::env::var("QMAKE").unwrap_or_else(|_| {
        if Command::new("qmake6").arg("-v").output().is_ok() {
            "qmake6".into()
        } else {
            "qmake".into()
        }
    });
    let libs = Command::new(&qmake)
        .args(["-query", "QT_INSTALL_LIBS"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|libs| !libs.is_empty());
    match libs {
        Some(libs) => println!("cargo:rustc-link-arg=-Wl,-rpath,{libs}"),
        None => {
            println!("cargo:warning=qmake did not say where Qt's libraries are: no run path added")
        }
    }
}
