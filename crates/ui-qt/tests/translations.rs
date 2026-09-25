// SPDX-License-Identifier: GPL-3.0-or-later
//! The translation files (`i18n/auroraw_*.ts`, Qt Linguist) against the QML: every string the screens
//! mark with `qsTr` is in each file, nothing else is, every translation is finished and keeps the
//! placeholders (`%1`, `%n`) of its source. English is the source text, so it has no file.

use std::collections::BTreeSet;
use std::path::Path;

fn read_dir(name: &str, extension: &str) -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == extension))
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

/// The texts of every `qsTr("...")` call.
fn qml_strings() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for (_, text) in read_dir("qml", "qml") {
        let mut rest = text.as_str();
        while let Some(at) = rest.find("qsTr(\"") {
            rest = &rest[at + 6..];
            let mut value = String::new();
            let mut chars = rest.chars();
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' => match chars.next() {
                        Some('n') => value.push('\n'),
                        Some(other) => value.push(other),
                        None => break,
                    },
                    other => value.push(other),
                }
            }
            found.insert(value);
        }
    }
    found
}

fn unescape(xml: &str) -> String {
    xml.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// `(source, translation element, translation text)` of every message of a `.ts`.
fn messages(ts: &str) -> Vec<(String, String, String)> {
    ts.split("<message")
        .skip(1)
        .map(|message| {
            let between = |open: &str, close: &str| {
                let start = message.find(open).unwrap() + open.len();
                let end = start + message[start..].find(close).unwrap();
                message[start..end].to_string()
            };
            let source = unescape(&between("<source>", "</source>"));
            let element_start = message.find("<translation").unwrap();
            let element_end = element_start + message[element_start..].find('>').unwrap();
            let element = message[element_start..=element_end].to_string();
            let text = unescape(&between(&element, "</translation>"));
            (source, element, text)
        })
        .collect()
}

fn placeholders(text: &str) -> Vec<String> {
    let mut found: Vec<String> = text
        .match_indices('%')
        .filter_map(|(at, _)| {
            let next = text[at + 1..].chars().next()?;
            (next.is_ascii_digit() || next == 'n').then(|| format!("%{next}"))
        })
        .collect();
    found.sort();
    found
}

#[test]
fn every_translation_file_covers_the_screens_exactly_and_is_finished() {
    let wanted = qml_strings();
    assert!(!wanted.is_empty(), "the QML has translatable strings");
    let files = read_dir("i18n", "ts");
    assert!(!files.is_empty(), "there is at least one translation");
    for (name, ts) in files {
        let messages = messages(&ts);
        let have: BTreeSet<String> = messages.iter().map(|(s, _, _)| s.clone()).collect();
        assert_eq!(
            wanted.difference(&have).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "{name}: strings of the screens missing from the file (run lupdate)"
        );
        assert_eq!(
            have.difference(&wanted).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "{name}: strings no screen uses any more (run lupdate -no-obsolete)"
        );
        for (source, element, text) in messages {
            assert!(
                !element.contains("type="),
                "{name}: {source:?} is {element}: not finished"
            );
            assert!(
                !text.trim().is_empty(),
                "{name}: {source:?} has no translation"
            );
            assert_eq!(
                placeholders(&source),
                placeholders(&text),
                "{name}: {source:?} and its translation differ in their placeholders"
            );
        }
    }
}
