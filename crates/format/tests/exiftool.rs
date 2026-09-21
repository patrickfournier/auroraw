// SPDX-License-Identifier: GPL-3.0-or-later
//! Interoperability: ExifTool reads back what Auroraw writes (M1 exit criterion 8, design note 003).
//!
//! Runs when `exiftool` is on the path. Continuous integration installs it and sets
//! `AUR_REQUIRE_EXIFTOOL=1`, so that a missing tool there is a failure, not a silent skip.

use std::path::Path;
use std::process::Command;

fn exiftool(args: &[&str], file: &Path) -> Option<String> {
    match Command::new("exiftool").args(args).arg(file).output() {
        Ok(out) => Some(String::from_utf8_lossy(&out.stdout).into_owned()),
        Err(_) if std::env::var_os("AUR_REQUIRE_EXIFTOOL").is_none() => {
            eprintln!("exiftool not found: skipping");
            None
        }
        Err(e) => panic!("exiftool is required here but could not be run: {e}"),
    }
}

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sidecars")
        .join(name)
}

/// ExifTool prints a one-item list as a plain value.
fn list(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::Array(items) => items
            .iter()
            .map(|i| i.as_str().unwrap_or_default().to_string())
            .collect(),
        other => vec![other.as_str().unwrap_or_default().to_string()],
    }
}

#[test]
fn a_photo_sidecar_validates_and_is_read_where_other_software_looks() {
    let file = fixture("photo-full.xmp");
    let Some(check) = exiftool(&["-validate", "-warning", "-error", "-a", "-G1"], &file) else {
        return;
    };
    assert!(
        check.contains("Validate") && check.contains(": OK"),
        "{check}"
    );
    assert!(
        !check.contains("Warning") && !check.contains("Error"),
        "{check}"
    );

    let json = exiftool(&["-json", "-G1"], &file).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let tags = &value[0];
    assert_eq!(tags["XMP-xmp:Rating"], 4);
    assert_eq!(tags["XMP-xmp:Label"], "Green");
    assert_eq!(tags["XMP-dc:Title"], "Heron at dawn");
    assert_eq!(tags["XMP-dc:Description"], "A grey heron & its reflection");
    assert_eq!(list(&tags["XMP-dc:Subject"]), ["Heron", "Quebec"]);
    assert_eq!(
        list(&tags["XMP-lr:HierarchicalSubject"]),
        ["Fauna|Birds|Heron", "Places|Canada|Quebec"]
    );
    assert_eq!(list(&tags["XMP-dc:Creator"]), ["Marie Tremblay"]);
    assert_eq!(tags["XMP-photoshop:City"], "Roberval");
    assert_eq!(tags["XMP-tiff:Make"], "SONY");
    assert_eq!(tags["XMP-aur:PhotoId"], "3f2a91c0d77e4b5a8c1e0f9d2b6a4c31");
}

#[test]
fn a_version_sidecar_shows_its_own_values_to_other_software() {
    let file = fixture("version-overrides.xmp");
    let Some(json) = exiftool(&["-json", "-G1"], &file) else {
        return;
    };
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value[0]["XMP-xmp:Rating"], 1);
    assert_eq!(value[0]["XMP-dc:Title"], "Its own title");
    assert_eq!(
        value[0]["XMP-dc:Description"],
        "A grey heron & its reflection"
    );
}
