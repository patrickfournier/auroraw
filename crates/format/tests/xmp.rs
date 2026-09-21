// SPDX-License-Identifier: GPL-3.0-or-later
//! XMP reader and writer: equivalent forms, canonical output, unknown content, errors, and a
//! generated round trip (testing strategy §3, design note 003 §9).

use auroraw_format::xmp::{ArrayKind, Item, Property, Value, Xmp, ns};
use proptest::prelude::*;

const ELEMENT_FORM: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
        xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/">
      <xmp:Rating>4</xmp:Rating>
      <dc:title><rdf:Alt><rdf:li xml:lang="x-default">A &amp; B &lt;3</rdf:li></rdf:Alt></dc:title>
      <dc:subject><rdf:Bag><rdf:li>Heron</rdf:li><rdf:li>Lake</rdf:li></rdf:Bag></dc:subject>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;

/// The same information as Lightroom writes it: properties as attributes, several descriptions.
const ATTRIBUTE_FORM: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 5.6-c140">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="4"/>
  <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/">
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">A &amp; B &lt;3</rdf:li></rdf:Alt></dc:title>
   <dc:subject>
    <rdf:Bag>
     <rdf:li>Heron</rdf:li>
     <rdf:li>Lake</rdf:li>
    </rdf:Bag>
   </dc:subject>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

#[test]
fn equivalent_forms_read_to_the_same_model_and_the_same_bytes() {
    let a = Xmp::from_bytes(ELEMENT_FORM.as_bytes()).unwrap();
    let b = Xmp::from_bytes(ATTRIBUTE_FORM.as_bytes()).unwrap();
    assert_eq!(a.properties, b.properties);
    assert_eq!(a.to_bytes(), b.to_bytes());
    assert_eq!(a.get(ns::XMP, "Rating").unwrap().as_text(), Some("4"));
    assert_eq!(
        a.get(ns::DC, "title").unwrap().as_lang_text(),
        Some("A & B <3")
    );
    assert_eq!(
        a.get(ns::DC, "subject").unwrap().as_texts(),
        Some(vec!["Heron", "Lake"])
    );
}

#[test]
fn the_canonical_form_is_stable() {
    let xmp = Xmp::from_bytes(ELEMENT_FORM.as_bytes()).unwrap();
    let once = xmp.to_bytes();
    let again = Xmp::from_bytes(&once).unwrap().to_bytes();
    assert_eq!(once, again);
    let text = String::from_utf8(once).unwrap();
    assert!(text.starts_with("<?xpacket begin=\"\u{FEFF}\""));
    assert!(text.contains("<xmp:Rating>4</xmp:Rating>"), "{text}");
    assert!(
        text.contains("<rdf:li xml:lang=\"x-default\">A &amp; B &lt;3</rdf:li>"),
        "{text}"
    );
    assert!(text.ends_with("<?xpacket end=\"w\"?>\n"));
    assert!(!text.contains('\r'));
}

#[test]
fn structures_in_every_notation() {
    let doc = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description xmlns:t="urn:test:">
        <t:a rdf:parseType="Resource"><t:x>1</t:x><t:y>2</t:y></t:a>
        <t:b><rdf:Description t:x="1" t:y="2"/></t:b>
        <t:c t:x="1" t:y="2"/>
        <t:d><rdf:Seq><rdf:li rdf:parseType="Resource"><t:x>1</t:x></rdf:li><rdf:li><rdf:Description t:x="2"/></rdf:li></rdf:Seq></t:d>
        <t:e rdf:resource="http://example.org/"/>
      </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let xmp = Xmp::from_bytes(doc.as_bytes()).unwrap();
    let field = |p: &Property, n: &str| {
        p.as_fields()
            .unwrap()
            .iter()
            .find(|f| f.name == n)
            .unwrap()
            .as_text()
            .map(str::to_string)
    };
    let get = |n: &str| xmp.get("urn:test:", n).unwrap();
    for n in ["a", "b", "c"] {
        assert_eq!(field(get(n), "x").as_deref(), Some("1"), "{n}");
    }
    assert_eq!(field(get("a"), "y").as_deref(), Some("2"));
    match &get("d").value {
        Value::Array(ArrayKind::Seq, items) => assert_eq!(items.len(), 2),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        get("e").value,
        Value::Resource("http://example.org/".into())
    );
    // and all of it survives a write and a read
    assert_eq!(
        Xmp::from_bytes(&xmp.to_bytes()).unwrap().properties,
        xmp.properties
    );
}

#[test]
fn unknown_namespaces_and_their_prefixes_are_kept() {
    let doc = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Exposure2012="+0.35" crs:Temperature="5200">
        <crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012>
      </rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let xmp = Xmp::from_bytes(doc.as_bytes()).unwrap();
    assert_eq!(xmp.properties.len(), 3);
    let text = String::from_utf8(xmp.to_bytes()).unwrap();
    assert!(
        text.contains("xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\""),
        "{text}"
    );
    assert!(
        text.contains("<crs:Exposure2012>+0.35</crs:Exposure2012>"),
        "{text}"
    );
    let again = Xmp::from_bytes(text.as_bytes()).unwrap();
    assert_eq!(again.properties, xmp.properties);
}

#[test]
fn a_prefix_that_clashes_with_a_known_one_is_renamed_not_confused() {
    let doc = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description xmlns:dc="urn:not-dublin-core:"><dc:title>t</dc:title></rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let xmp = Xmp::from_bytes(doc.as_bytes()).unwrap();
    let again = Xmp::from_bytes(&xmp.to_bytes()).unwrap();
    assert_eq!(again.properties[0].ns, "urn:not-dublin-core:");
}

#[test]
fn characters_that_need_care_survive() {
    let mut xmp = Xmp::default();
    for (i, text) in [
        "  leading and trailing  ",
        "line\none\r\ntwo\ttab",
        "<&>\"'",
        "héron 鷺 🦢",
        "",
    ]
    .iter()
    .enumerate()
    {
        xmp.properties
            .push(Property::text(ns::DC, &format!("p{i}"), *text));
    }
    let back = Xmp::from_bytes(&xmp.to_bytes()).unwrap();
    assert_eq!(back.properties, xmp.properties);
}

#[test]
fn byte_order_mark_and_missing_wrapper_are_accepted() {
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(ATTRIBUTE_FORM.as_bytes());
    assert_eq!(Xmp::from_bytes(&bytes).unwrap().properties.len(), 3);
    let bare = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="2"/></rdf:RDF>"#;
    assert_eq!(
        Xmp::from_bytes(bare.as_bytes()).unwrap().properties.len(),
        1
    );
}

#[test]
fn bad_input_is_an_error() {
    for bad in [
        &b""[..],
        b"not xml",
        b"<a/>",
        b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description><p:q>1</p:q></rdf:Description></rdf:RDF></x:xmpmeta>",
        b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description xmlns:t=\"urn:t:\"><t:a>1</t:a>",
        b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description xmlns:t=\"urn:t:\"><t:a>&nosuch;</t:a></rdf:Description></rdf:RDF></x:xmpmeta>",
    ] {
        assert!(Xmp::from_bytes(bad).is_err(), "{:?}", String::from_utf8_lossy(bad));
    }
}

#[test]
fn deep_nesting_is_refused_not_crashed() {
    let mut doc = String::from(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:t="urn:t:">"#,
    );
    for _ in 0..200 {
        doc.push_str("<t:a>");
    }
    assert!(Xmp::from_bytes(doc.as_bytes()).is_err());
}

// ---- a generated round trip: any model that can be written reads back the same ----

fn name() -> impl Strategy<Value = String> {
    "[A-Za-z][A-Za-z0-9]{0,6}"
}

fn uri() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(ns::DC.to_string()),
        Just(ns::XMP.to_string()),
        Just(ns::AUR.to_string()),
        Just("urn:example:one:".to_string()),
        Just("urn:example:two:".to_string()),
    ]
}

fn text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            4 => proptest::char::range('a', 'z'),
            1 => Just(' '), 1 => Just('&'), 1 => Just('<'), 1 => Just('>'), 1 => Just('"'),
            1 => Just('\n'), 1 => Just('\r'), 1 => Just('\t'), 1 => Just('é'), 1 => Just('鷺'), 1 => Just('🦢'),
        ],
        0..12,
    )
    .prop_map(|c| c.into_iter().collect())
}

fn lang() -> impl Strategy<Value = Option<String>> {
    proptest::option::of(prop_oneof![
        Just("x-default".to_string()),
        Just("fr-CA".to_string())
    ])
}

fn leaf() -> impl Strategy<Value = Value> {
    prop_oneof![
        text().prop_map(Value::Text),
        "[a-z]{1,8}".prop_map(|s| Value::Resource(format!("http://e.org/{s}")))
    ]
}

fn value() -> impl Strategy<Value = Value> {
    // A language only makes sense on text.
    let item = (lang(), leaf()).prop_map(|(lang, value)| {
        let lang = if matches!(value, Value::Text(_)) {
            lang
        } else {
            None
        };
        Item { lang, value }
    });
    let kind = prop_oneof![
        Just(ArrayKind::Seq),
        Just(ArrayKind::Bag),
        Just(ArrayKind::Alt)
    ];
    prop_oneof![
        3 => leaf(),
        2 => (kind, proptest::collection::vec(item, 0..4)).prop_map(|(k, i)| Value::Array(k, i)),
        1 => proptest::collection::vec(
            (uri(), name(), leaf()).prop_map(|(ns, name, value)| Property { ns, name, lang: None, value }), 0..3
        ).prop_map(Value::Struct),
    ]
}

fn property() -> impl Strategy<Value = Property> {
    (uri(), name(), lang(), value()).prop_map(|(ns, name, lang, value)| {
        let lang = if matches!(value, Value::Text(_)) {
            lang
        } else {
            None
        };
        Property {
            ns,
            name,
            lang,
            value,
        }
    })
}

proptest! {
    #[test]
    fn a_written_model_reads_back_identically_and_rewrites_identically(props in proptest::collection::vec(property(), 0..6)) {
        let xmp = Xmp { properties: props, prefixes: vec![] };
        let bytes = xmp.to_bytes();
        let back = Xmp::from_bytes(&bytes).unwrap();
        prop_assert_eq!(&back.properties, &xmp.properties);
        prop_assert_eq!(back.to_bytes(), bytes);
    }

    #[test]
    fn reading_arbitrary_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
        let _ = Xmp::from_bytes(&bytes);
    }

    #[test]
    fn reading_mutated_xmp_never_panics(at in 0usize..ELEMENT_FORM.len(), byte in any::<u8>()) {
        let mut b = ELEMENT_FORM.as_bytes().to_vec();
        b[at] = byte;
        let _ = Xmp::from_bytes(&b);
    }
}

// ---- XMP as other software writes it (design note 003 §9; the merge of §8.1 will rely on this) ----

fn foreign(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/xmp/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn lightroom_and_darktable_files_are_read_and_kept_whole() {
    for name in ["lightroom.xmp", "darktable.xmp"] {
        let xmp = Xmp::from_bytes(&foreign(name)).unwrap();
        assert!(!xmp.properties.is_empty(), "{name}");
        let back = Xmp::from_bytes(&xmp.to_bytes()).unwrap();
        assert_eq!(
            back.properties, xmp.properties,
            "{name}: nothing lost through a rewrite"
        );
        assert_eq!(back.to_bytes(), xmp.to_bytes(), "{name}: canonical");
    }
}

#[test]
fn develop_settings_of_other_software_are_all_there_after_a_rewrite() {
    let crs = "http://ns.adobe.com/camera-raw-settings/1.0/";
    let xmp = Xmp::from_bytes(&foreign("lightroom.xmp")).unwrap();
    let count = |x: &Xmp| x.properties.iter().filter(|p| p.ns == crs).count();
    assert_eq!(count(&xmp), 9);
    let back = Xmp::from_bytes(&xmp.to_bytes()).unwrap();
    assert_eq!(count(&back), 9);
    assert_eq!(
        back.get(crs, "Exposure2012").unwrap().as_text(),
        Some("+0.35")
    );
    assert_eq!(
        back.get(crs, "ToneCurvePV2012")
            .unwrap()
            .as_texts()
            .unwrap()
            .len(),
        3
    );

    let dt = Xmp::from_bytes(&foreign("darktable.xmp")).unwrap();
    let history = dt.get("http://darktable.sf.net/", "history").unwrap();
    match &history.value {
        Value::Array(ArrayKind::Seq, items) => {
            assert_eq!(items.len(), 2);
            let again = Xmp::from_bytes(&dt.to_bytes()).unwrap();
            assert_eq!(
                again
                    .get("http://darktable.sf.net/", "history")
                    .unwrap()
                    .value,
                history.value
            );
        }
        other => panic!("{other:?}"),
    }
}

// ---- hostile input: what RUSTSEC-2026-0194 and 0195 were about (quick-xml before 0.40) ----

#[test]
fn a_tag_with_thousands_of_attributes_is_read_in_bounded_time() {
    let mut doc = String::from(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:t="urn:t:""#,
    );
    for i in 0..20_000 {
        doc.push_str(&format!(" t:a{i}=\"1\""));
    }
    doc.push_str("/></rdf:RDF></x:xmpmeta>");
    let started = std::time::Instant::now();
    let _ = Xmp::from_bytes(doc.as_bytes());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn thousands_of_namespace_declarations_are_refused_not_allocated() {
    let mut doc = String::from(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description"#,
    );
    for i in 0..50_000 {
        doc.push_str(&format!(" xmlns:p{i}=\"urn:p:{i}\""));
    }
    doc.push_str("/></rdf:RDF></x:xmpmeta>");
    assert!(Xmp::from_bytes(doc.as_bytes()).is_err());
}

// ---- found by the nightly fuzzer ----

#[test]
fn a_namespace_with_an_entity_in_its_address_is_the_same_namespace_once_written_and_read() {
    // The reader hands back namespace addresses as written, with `&gt;` unresolved; the model
    // must hold `>`, or every rewrite would escape the ampersand once more.
    let doc = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description xmlns:t="urn:a&amp;b&gt;c:"><t:p>1</t:p></rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let xmp = Xmp::from_bytes(doc.as_bytes()).unwrap();
    assert_eq!(xmp.properties[0].ns, "urn:a&b>c:");
    let once = xmp.to_bytes();
    let back = Xmp::from_bytes(&once).unwrap();
    assert_eq!(back.properties[0].ns, "urn:a&b>c:");
    assert_eq!(back.to_bytes(), once);
}

#[test]
fn names_that_cannot_be_written_are_refused_when_read() {
    // Found by the nightly fuzzer: an attribute named `multi_priorit/` was accepted, then written
    // as an element that no reader can parse. What is accepted must be writable.
    let doc = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description xmlns:t="urn:t:"><t:a t:bad/name="1"/></rdf:Description></rdf:RDF></x:xmpmeta>"#;
    assert!(Xmp::from_bytes(doc.as_bytes()).is_err());
    let doc = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description xmlns:t="urn:t:"><t:a:b>1</t:a:b></rdf:Description></rdf:RDF></x:xmpmeta>"#;
    assert!(Xmp::from_bytes(doc.as_bytes()).is_err());
}

#[test]
fn stray_control_characters_are_replaced_when_read_so_that_what_is_read_can_be_written() {
    // Found by the nightly fuzzer: a control character in a namespace address was read as it was
    // but written as U+FFFD, so a second write differed from the first.
    let doc = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
      <rdf:Description xmlns:t=\"urn:a\u{1f}b:\"><t:p t:q=\"x\u{0}y\">caption\u{b}here</t:p></rdf:Description></rdf:RDF></x:xmpmeta>";
    let xmp = Xmp::from_bytes(doc.as_bytes()).unwrap();
    assert_eq!(xmp.properties[0].ns, "urn:a\u{FFFD}b:");
    assert_eq!(xmp.properties[0].as_text(), Some("caption\u{FFFD}here"));
    let once = xmp.to_bytes();
    assert_eq!(Xmp::from_bytes(&once).unwrap().to_bytes(), once);
}

#[test]
fn control_characters_in_attribute_values_are_replaced_too() {
    let doc = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
      <rdf:Description xmlns:t=\"urn:t:\" t:q=\"x\u{0}y\"/></rdf:RDF></x:xmpmeta>";
    let xmp = Xmp::from_bytes(doc.as_bytes()).unwrap();
    assert_eq!(xmp.properties[0].as_text(), Some("x\u{FFFD}y"));
}
