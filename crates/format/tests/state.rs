// SPDX-License-Identifier: GPL-3.0-or-later
//! State files: fixtures, canonical bytes, unknown content, newer schemas (testing strategy §3).

use auroraw_format::state::*;
use proptest::prelude::*;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/state/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// Reads a fixture, writes it back, and requires the same bytes: the fixtures are canonical.
fn canonical<T: StateBody + std::fmt::Debug>(name: &str) -> T {
    let bytes = fixture(name);
    let value = read_state::<T>(&bytes)
        .unwrap()
        .current()
        .expect("current schema");
    assert_eq!(
        String::from_utf8(write_state(&value)).unwrap(),
        String::from_utf8(bytes).unwrap(),
        "{name}"
    );
    value
}

#[test]
fn every_fixture_is_read_and_written_back_identically() {
    let marker: Marker = canonical("workspace.json");
    assert_eq!(marker.layout, 1);
    assert_eq!(marker.catalogue_name, "Main");
    let vocabulary: Vocabulary = canonical("vocabulary.json");
    assert_eq!(vocabulary.keywords.len(), 2);
    assert!(!vocabulary.keywords[1].export);
    let sources: Sources = canonical("sources.json");
    assert_eq!(sources.sources[0].kind, "local-folder");
    let manual: Collection = canonical("collection-manual.json");
    assert_eq!(manual.members.len(), 2);
    let smart: Collection = canonical("collection-smart.json");
    assert_eq!(smart.query_schema, Some(1));
    let series: Series = canonical("series.json");
    assert_eq!(series.members.len(), 2);
}

#[test]
fn the_vocabulary_is_written_sorted_by_identifier() {
    let mut v: Vocabulary = read_state(&fixture("vocabulary.json"))
        .unwrap()
        .current()
        .unwrap();
    v.keywords.reverse();
    assert_eq!(v.to_bytes(), fixture("vocabulary.json"));
}

#[test]
fn unknown_keys_survive_at_every_level() {
    let text = r#"{
      "format": "auroraw/vocabulary", "schema": 1, "updated": "2026-09-21T14:02:11Z",
      "future_top": {"a": [1, 2]},
      "keywords": [
        {"id": "0a1b2c3d4e5f6071", "name": "Fauna", "parent": null, "future_entry": "kept"}
      ]
    }"#;
    let v: Vocabulary = read_state(text.as_bytes()).unwrap().current().unwrap();
    let out = String::from_utf8(v.to_bytes()).unwrap();
    assert!(out.contains("\"future_top\""), "{out}");
    assert!(out.contains("\"future_entry\": \"kept\""), "{out}");
    // and what is written is read back the same
    let again: Vocabulary = read_state(out.as_bytes()).unwrap().current().unwrap();
    assert_eq!(again, v);
}

#[test]
fn a_newer_schema_is_reported_and_not_interpreted() {
    let text = r#"{"format": "auroraw/series", "schema": 9, "anything": [1]}"#;
    match read_state::<Series>(text.as_bytes()).unwrap() {
        Loaded::Newer(s) => assert_eq!(s.0, 9),
        Loaded::Current(_) => panic!("must not be interpreted"),
    }
}

#[test]
fn another_format_or_a_broken_file_is_an_error() {
    let text = fixture("series.json");
    assert!(matches!(
        read_state::<Collection>(&text),
        Err(StateError::Format { .. })
    ));
    assert!(read_state::<Series>(b"{").is_err());
    assert!(read_state::<Series>(b"[]").is_err());
    assert!(read_state::<Series>(br#"{"format": "auroraw/series"}"#).is_err());
    // a wrong shape inside a current schema is an error, never a silent default
    assert!(read_state::<Series>(br#"{"format":"auroraw/series","schema":1,"id":"zz"}"#).is_err());
}

#[test]
fn a_byte_order_mark_is_tolerated_on_reading_and_never_written() {
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend(fixture("series.json"));
    let s: Series = read_state(&bytes).unwrap().current().unwrap();
    assert!(!write_state(&s).starts_with(b"\xEF\xBB\xBF"));
}

#[test]
fn the_same_content_gives_the_same_bytes() {
    let s: Series = read_state(&fixture("series.json"))
        .unwrap()
        .current()
        .unwrap();
    assert_eq!(write_state(&s), write_state(&s.clone()));
}

proptest! {
    #[test]
    fn reading_arbitrary_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        let _ = read_state::<Vocabulary>(&bytes);
        let _ = read_state::<Collection>(&bytes);
        let _ = read_state::<Series>(&bytes);
        let _ = read_state::<Sources>(&bytes);
        let _ = read_state::<Marker>(&bytes);
    }
}
