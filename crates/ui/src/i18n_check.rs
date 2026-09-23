// SPDX-License-Identifier: GPL-3.0-or-later
//! A check that no `@tr()` string in the shell is missing from a translation, and no translated
//! string is left over from one that no longer exists (testing strategy §6: "a check that no
//! message id is missing or unused"). Reads the source files directly rather than the compiled
//! bundle, so it catches a mismatch even before the pseudo-locale itself exists.

use std::collections::BTreeSet;
use std::path::Path;

/// Every literal inside `@tr("...")` in `source` (the ordinary case: this shell's strings have no
/// escaped quotes or interpolation braces inside the literal itself, only after it).
fn tr_strings(source: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = source;
    while let Some(at) = rest.find("@tr(\"") {
        rest = &rest[at + 5..];
        let Some(end) = rest.find('"') else { break };
        out.insert(rest[..end].to_string());
        rest = &rest[end + 1..];
    }
    out
}

/// Every `msgid` in a `.po` file, excluding the empty header entry.
fn po_msgids(source: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("msgid \"")
            && let Some(end) = rest.find('"')
        {
            let id = &rest[..end];
            if !id.is_empty() {
                out.insert(id.to_string());
            }
        }
    }
    out
}

fn manifest_path(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// Every `.slint` file of the shell, joined: the strings may be in any of them.
fn all_slint() -> String {
    let mut files: Vec<_> = std::fs::read_dir(manifest_path("ui"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "slint"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_translatable_string_has_an_english_and_a_french_entry() {
    let shell = all_slint();
    let used = tr_strings(&shell);
    assert!(
        !used.is_empty(),
        "sanity: the shell should use @tr() somewhere"
    );

    for lang in ["en", "fr"] {
        let po = std::fs::read_to_string(manifest_path(&format!(
            "i18n/{lang}/LC_MESSAGES/auroraw-ui.po"
        )))
        .unwrap();
        let ids = po_msgids(&po);
        let missing: Vec<&String> = used.difference(&ids).collect();
        assert!(
            missing.is_empty(),
            "{lang}: missing from auroraw-ui.po: {missing:?}"
        );
        let unused: Vec<&String> = ids.difference(&used).collect();
        assert!(
            unused.is_empty(),
            "{lang}: auroraw-ui.po has an entry the shell no longer uses: {unused:?}"
        );
    }
}
