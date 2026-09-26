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

/// One message of a `.ts` file.
struct Message {
    source: String,
    /// It has plural forms (`%n`).
    numerus: bool,
    /// The translation is marked neither unfinished nor vanished.
    finished: bool,
    /// The translation's text: one entry, or one per plural form.
    forms: Vec<String>,
}

fn messages(ts: &str) -> Vec<Message> {
    ts.split("<message")
        .skip(1)
        .map(|message| {
            let between = |open: &str, close: &str, from: usize| {
                let start = from + message[from..].find(open).unwrap() + open.len();
                let end = start + message[start..].find(close).unwrap();
                message[start..end].to_string()
            };
            let source = unescape(&between("<source>", "</source>", 0));
            let numerus = message.trim_start().starts_with("numerus=\"yes\"");
            let at = message.find("<translation").unwrap();
            let tag_end = at + message[at..].find('>').unwrap();
            let tag = &message[at..=tag_end];
            let finished = !tag.contains("type=");
            let forms = if tag.ends_with("/>") {
                Vec::new()
            } else if numerus {
                message[tag_end..]
                    .split("<numerusform")
                    .skip(1)
                    .map(|form| {
                        let text = &form[form.find('>').unwrap() + 1..];
                        unescape(&text[..text.find("</numerusform>").unwrap_or(text.len())])
                    })
                    .collect()
            } else {
                let text = &message[tag_end + 1..];
                vec![unescape(
                    &text[..text.find("</translation>").unwrap_or(text.len())],
                )]
            };
            Message {
                source,
                numerus,
                finished,
                forms,
            }
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
        // English is what the screens are written in: its file only carries the plural forms, which
        // a source text cannot hold (`%n photo(s)`), and the rest of it stays as lupdate leaves it.
        let source_language = ts.contains(" language=\"en\"");
        let messages = messages(&ts);
        let have: BTreeSet<String> = messages.iter().map(|m| m.source.clone()).collect();
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
        for message in messages {
            if source_language && !message.numerus {
                continue;
            }
            let source = &message.source;
            assert!(message.finished, "{name}: {source:?} is not finished");
            assert!(
                !message.forms.is_empty() && message.forms.iter().all(|f| !f.trim().is_empty()),
                "{name}: {source:?} has no translation"
            );
            if message.numerus {
                // A plural form may leave %n out ("one photo"), but adds no other placeholder.
                assert!(
                    message.forms.len() >= 2 || source_language,
                    "{name}: {source:?} has one plural form"
                );
                for form in &message.forms {
                    for placeholder in placeholders(form) {
                        assert!(
                            placeholders(source).contains(&placeholder),
                            "{name}: a form of {source:?} has {placeholder}, which the source lacks"
                        );
                    }
                }
            } else {
                assert_eq!(
                    placeholders(source),
                    placeholders(&message.forms[0]),
                    "{name}: {source:?} and its translation differ in their placeholders"
                );
            }
        }
    }
}
