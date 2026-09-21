// SPDX-License-Identifier: GPL-3.0-or-later
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;

use super::model::{ArrayKind, Item, Property, Value, Xmp};
use super::names::ns;

/// Why an XMP document could not be read.
#[derive(Debug, thiserror::Error)]
pub enum XmpError {
    /// The XML is malformed.
    #[error("malformed XML: {0}")]
    Xml(String),
    /// A prefix is used without being declared.
    #[error("undeclared namespace prefix {0:?}")]
    UnboundPrefix(String),
    /// The document is not XMP (no `rdf:RDF` element).
    #[error("not an XMP document: no rdf:RDF element")]
    NotXmp,
    /// A construct that cannot be represented; the file is refused rather than damaged.
    #[error("unsupported XMP construct: {0}")]
    Unsupported(String),
    /// The document is nested too deeply.
    #[error("the XMP document is nested too deeply")]
    TooDeep,
}

const MAX_DEPTH: usize = 64;

struct El {
    ns: Option<String>,
    local: String,
    attrs: Vec<Attr>,
    children: Vec<Node>,
}

struct Attr {
    ns: Option<String>,
    local: String,
    value: String,
}

enum Node {
    El(El),
    Text(String),
}

fn xml_err(e: impl std::fmt::Display) -> XmpError {
    XmpError::Xml(e.to_string())
}

fn resolved(r: ResolveResult<'_>) -> Result<Option<String>, XmpError> {
    match r {
        ResolveResult::Bound(n) => Ok(Some(String::from_utf8_lossy(n.as_ref()).into_owned())),
        ResolveResult::Unbound => Ok(None),
        ResolveResult::Unknown(p) => Err(XmpError::UnboundPrefix(
            String::from_utf8_lossy(&p).into_owned(),
        )),
    }
}

fn make_element(
    reader: &NsReader<&[u8]>,
    start: &BytesStart<'_>,
    element_ns: Option<String>,
    prefixes: &mut Vec<(String, String)>,
) -> Result<El, XmpError> {
    let local = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
    let mut attrs = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(xml_err)?;
        let key = attribute.key.as_ref();
        let value = attribute.unescape_value().map_err(xml_err)?.into_owned();
        if key == b"xmlns" {
            continue;
        }
        if let Some(prefix) = key.strip_prefix(b"xmlns:") {
            let prefix = String::from_utf8_lossy(prefix).into_owned();
            if !prefixes.iter().any(|(p, u)| *p == prefix && *u == value) {
                prefixes.push((prefix, value));
            }
            continue;
        }
        let (res, local) = reader.resolve_attribute(attribute.key);
        attrs.push(Attr {
            ns: resolved(res)?,
            local: String::from_utf8_lossy(local.as_ref()).into_owned(),
            value,
        });
    }
    Ok(El {
        ns: element_ns,
        local,
        attrs,
        children: Vec::new(),
    })
}

fn push_text(stack: &mut [El], text: &str) {
    if let Some(top) = stack.last_mut() {
        match top.children.last_mut() {
            Some(Node::Text(t)) => t.push_str(text),
            _ => top.children.push(Node::Text(text.to_string())),
        }
    }
}

fn parse_dom(bytes: &[u8], prefixes: &mut Vec<(String, String)>) -> Result<El, XmpError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let mut reader = NsReader::from_reader(bytes);
    let mut stack: Vec<El> = Vec::new();
    let mut root: Option<El> = None;
    loop {
        let (res, event) = reader.read_resolved_event().map_err(xml_err)?;
        let element_ns = match &event {
            Event::Start(_) | Event::Empty(_) => resolved(res)?,
            _ => None,
        };
        match event {
            Event::Start(start) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(XmpError::TooDeep);
                }
                stack.push(make_element(&reader, &start, element_ns, prefixes)?);
            }
            Event::Empty(start) => {
                let el = make_element(&reader, &start, element_ns, prefixes)?;
                match stack.last_mut() {
                    Some(top) => top.children.push(Node::El(el)),
                    None => root = root.or(Some(el)),
                }
            }
            Event::End(_) => {
                let el = stack.pop().ok_or_else(|| xml_err("unexpected end tag"))?;
                match stack.last_mut() {
                    Some(top) => top.children.push(Node::El(el)),
                    None => root = root.or(Some(el)),
                }
            }
            Event::Text(t) => {
                let text = t.xml_content().map_err(xml_err)?;
                push_text(&mut stack, &text);
            }
            Event::CData(c) => {
                let text = c.decode().map_err(xml_err)?;
                push_text(&mut stack, &text);
            }
            Event::GeneralRef(r) => {
                let text = match r.resolve_char_ref().map_err(xml_err)? {
                    Some(c) => c.to_string(),
                    None => {
                        let name = r.decode().map_err(xml_err)?;
                        resolve_predefined_entity(&name)
                            .ok_or_else(|| xml_err(format!("unknown entity &{name};")))?
                            .to_string()
                    }
                };
                push_text(&mut stack, &text);
            }
            Event::Eof => break,
            Event::Comment(_) | Event::PI(_) | Event::Decl(_) | Event::DocType(_) => {}
        }
    }
    if !stack.is_empty() {
        return Err(xml_err("the document ends inside an element"));
    }
    root.ok_or(XmpError::NotXmp)
}

fn is_el(e: &El, namespace: &str, local: &str) -> bool {
    e.ns.as_deref() == Some(namespace) && e.local == local
}

fn attr<'a>(e: &'a El, namespace: &str, local: &str) -> Option<&'a str> {
    e.attrs
        .iter()
        .find(|a| a.ns.as_deref() == Some(namespace) && a.local == local)
        .map(|a| a.value.as_str())
}

fn lang(e: &El) -> Option<String> {
    attr(e, ns::XML, "lang").map(str::to_string)
}

fn child_elements(e: &El) -> impl Iterator<Item = &El> {
    e.children.iter().filter_map(|n| match n {
        Node::El(c) => Some(c),
        Node::Text(_) => None,
    })
}

fn text_of(e: &El) -> String {
    e.children
        .iter()
        .filter_map(|n| match n {
            Node::Text(t) => Some(t.as_str()),
            Node::El(_) => None,
        })
        .collect()
}

fn only_whitespace(s: &str) -> bool {
    s.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
}

/// Attributes that are properties: everything but `rdf:*` bookkeeping and `xml:*`.
fn attribute_properties(e: &El) -> impl Iterator<Item = &Attr> {
    e.attrs.iter().filter(|a| {
        a.ns.is_some() && a.ns.as_deref() != Some(ns::RDF) && a.ns.as_deref() != Some(ns::XML)
    })
}

fn fields_of(e: &El) -> Result<Vec<Property>, XmpError> {
    let mut fields = Vec::new();
    for a in attribute_properties(e) {
        fields.push(Property {
            ns: a.ns.clone().unwrap_or_default(),
            name: a.local.clone(),
            lang: None,
            value: Value::Text(a.value.clone()),
        });
    }
    for child in child_elements(e) {
        fields.push(property(child)?);
    }
    Ok(fields)
}

fn array_kind(e: &El) -> Option<ArrayKind> {
    if is_el(e, ns::RDF, "Seq") {
        Some(ArrayKind::Seq)
    } else if is_el(e, ns::RDF, "Bag") {
        Some(ArrayKind::Bag)
    } else if is_el(e, ns::RDF, "Alt") {
        Some(ArrayKind::Alt)
    } else {
        None
    }
}

fn value_of(e: &El) -> Result<Value, XmpError> {
    if attr(e, ns::RDF, "parseType") == Some("Resource") {
        return Ok(Value::Struct(fields_of(e)?));
    }
    if let Some(uri) = attr(e, ns::RDF, "resource") {
        return Ok(Value::Resource(uri.to_string()));
    }
    let children: Vec<&El> = child_elements(e).collect();
    let text = text_of(e);
    match children.as_slice() {
        [] => {
            if attribute_properties(e).next().is_some() && only_whitespace(&text) {
                // An empty property element whose attributes are the fields of a structure.
                Ok(Value::Struct(fields_of(e)?))
            } else {
                Ok(Value::Text(text))
            }
        }
        [only] => {
            if let Some(kind) = array_kind(only) {
                let mut items = Vec::new();
                for li in child_elements(only) {
                    if !is_el(li, ns::RDF, "li") {
                        return Err(XmpError::Unsupported(format!(
                            "{} inside an array",
                            li.local
                        )));
                    }
                    items.push(Item {
                        lang: lang(li),
                        value: item_value(li)?,
                    });
                }
                Ok(Value::Array(kind, items))
            } else if is_el(only, ns::RDF, "Description") {
                Ok(Value::Struct(fields_of(only)?))
            } else {
                Err(XmpError::Unsupported(format!(
                    "element {} inside a property",
                    only.local
                )))
            }
        }
        _ => Err(XmpError::Unsupported(
            "several elements inside a property".into(),
        )),
    }
}

fn item_value(li: &El) -> Result<Value, XmpError> {
    value_of(li)
}

fn property(e: &El) -> Result<Property, XmpError> {
    let namespace = e.ns.clone().ok_or_else(|| {
        XmpError::Unsupported(format!("property {} without a namespace", e.local))
    })?;
    if namespace == ns::RDF {
        return Err(XmpError::Unsupported(format!(
            "rdf:{} used as a property",
            e.local
        )));
    }
    let value = value_of(e)?;
    let lang = if matches!(value, Value::Text(_)) {
        lang(e)
    } else {
        None
    };
    Ok(Property {
        ns: namespace,
        name: e.local.clone(),
        lang,
        value,
    })
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Xmp, XmpError> {
    let mut prefixes = Vec::new();
    let root = parse_dom(bytes, &mut prefixes)?;
    let mut rdf_roots: Vec<&El> = Vec::new();
    if is_el(&root, ns::RDF, "RDF") {
        rdf_roots.push(&root);
    } else if is_el(&root, ns::META, "xmpmeta") {
        rdf_roots.extend(child_elements(&root).filter(|c| is_el(c, ns::RDF, "RDF")));
    }
    if rdf_roots.is_empty() {
        return Err(XmpError::NotXmp);
    }
    let mut properties = Vec::new();
    for rdf in rdf_roots {
        for description in child_elements(rdf) {
            if !is_el(description, ns::RDF, "Description") {
                return Err(XmpError::Unsupported(format!(
                    "{} inside rdf:RDF",
                    description.local
                )));
            }
            properties.extend(fields_of(description)?);
        }
    }
    Ok(Xmp {
        properties,
        prefixes,
    })
}
