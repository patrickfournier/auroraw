// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::BTreeMap;
use std::fmt::Write;

use super::model::{ArrayKind, Item, Property, Value, Xmp};
use super::names::{KNOWN, ns};

const INDENT: &str = "  ";

/// The prefix of each namespace used in the document, deterministically.
fn assign_prefixes(xmp: &Xmp) -> BTreeMap<String, String> {
    let mut used: Vec<&str> = Vec::new();
    fn collect<'a>(p: &'a Property, out: &mut Vec<&'a str>) {
        if !out.contains(&p.ns.as_str()) {
            out.push(&p.ns);
        }
        match &p.value {
            Value::Struct(fields) => fields.iter().for_each(|f| collect(f, out)),
            Value::Array(_, items) => {
                for i in items {
                    if let Value::Struct(fields) = &i.value {
                        fields.iter().for_each(|f| collect(f, out));
                    }
                }
            }
            _ => {}
        }
    }
    xmp.properties.iter().for_each(|p| collect(p, &mut used));

    let mut map: BTreeMap<String, String> = BTreeMap::new(); // uri -> prefix
    let mut taken: Vec<String> = ["rdf", "x", "xml", "xmlns"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for uri in &used {
        if let Some((prefix, _)) = KNOWN.iter().find(|(_, u)| u == uri) {
            map.insert((*uri).to_string(), (*prefix).to_string());
            taken.push((*prefix).to_string());
        }
    }
    let mut counter = 0;
    for uri in &used {
        if map.contains_key(*uri) {
            continue;
        }
        let hint = xmp
            .prefixes
            .iter()
            .find(|(p, u)| {
                u == uri
                    && is_ncname(p)
                    && !taken.contains(p)
                    && !p.to_lowercase().starts_with("xml")
            })
            .map(|(p, _)| p.clone());
        let prefix = hint.unwrap_or_else(|| {
            loop {
                counter += 1;
                let candidate = format!("ns{counter}");
                if !taken.contains(&candidate) {
                    break candidate;
                }
            }
        });
        taken.push(prefix.clone());
        map.insert((*uri).to_string(), prefix);
    }
    map
}

fn is_ncname(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// A character XML 1.0 allows.
fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

fn escape_text(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c if !xml_char(c) => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

fn escape_attribute(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\r' => out.push_str("&#13;"),
            '\n' => out.push_str("&#10;"),
            '\t' => out.push_str("&#9;"),
            c if !xml_char(c) => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

struct Writer<'a> {
    out: String,
    prefixes: &'a BTreeMap<String, String>,
}

impl Writer<'_> {
    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.out.push_str(INDENT);
        }
    }

    fn qname(&self, p: &Property) -> String {
        format!("{}:{}", self.prefixes[&p.ns], p.name)
    }

    fn lang_attribute(&mut self, lang: &Option<String>) {
        if let Some(l) = lang {
            self.out.push_str(" xml:lang=\"");
            escape_attribute(&mut self.out, l);
            self.out.push('"');
        }
    }

    fn property(&mut self, p: &Property, depth: usize) {
        let name = self.qname(p);
        self.indent(depth);
        match &p.value {
            Value::Text(t) => {
                let _ = write!(self.out, "<{name}");
                self.lang_attribute(&p.lang);
                self.out.push('>');
                escape_text(&mut self.out, t);
                let _ = writeln!(self.out, "</{name}>");
            }
            Value::Resource(uri) => {
                let _ = write!(self.out, "<{name} rdf:resource=\"");
                escape_attribute(&mut self.out, uri);
                self.out.push_str("\"/>\n");
            }
            Value::Struct(fields) => self.structure(&name, fields, depth, None),
            Value::Array(kind, items) => {
                let _ = writeln!(self.out, "<{name}>");
                self.array(*kind, items, depth + 1);
                self.indent(depth);
                let _ = writeln!(self.out, "</{name}>");
            }
        }
    }

    fn structure(&mut self, name: &str, fields: &[Property], depth: usize, lang: Option<&String>) {
        let _ = write!(self.out, "<{name}");
        if let Some(l) = lang {
            self.lang_attribute(&Some(l.clone()));
        }
        if fields.is_empty() {
            self.out.push_str(" rdf:parseType=\"Resource\"/>\n");
            return;
        }
        self.out.push_str(" rdf:parseType=\"Resource\">\n");
        for f in fields {
            self.property(f, depth + 1);
        }
        self.indent(depth);
        let _ = writeln!(self.out, "</{name}>");
    }

    fn array(&mut self, kind: ArrayKind, items: &[Item], depth: usize) {
        let tag = match kind {
            ArrayKind::Seq => "rdf:Seq",
            ArrayKind::Bag => "rdf:Bag",
            ArrayKind::Alt => "rdf:Alt",
        };
        self.indent(depth);
        if items.is_empty() {
            let _ = writeln!(self.out, "<{tag}/>");
            return;
        }
        let _ = writeln!(self.out, "<{tag}>");
        for item in items {
            self.item(item, depth + 1);
        }
        self.indent(depth);
        let _ = writeln!(self.out, "</{tag}>");
    }

    fn item(&mut self, item: &Item, depth: usize) {
        self.indent(depth);
        match &item.value {
            Value::Text(t) => {
                self.out.push_str("<rdf:li");
                self.lang_attribute(&item.lang);
                self.out.push('>');
                escape_text(&mut self.out, t);
                self.out.push_str("</rdf:li>\n");
            }
            Value::Resource(uri) => {
                self.out.push_str("<rdf:li rdf:resource=\"");
                escape_attribute(&mut self.out, uri);
                self.out.push_str("\"/>\n");
            }
            Value::Struct(fields) => self.structure("rdf:li", fields, depth, item.lang.as_ref()),
            Value::Array(kind, items) => {
                self.out.push_str("<rdf:li");
                self.lang_attribute(&item.lang);
                self.out.push_str(">\n");
                self.array(*kind, items, depth + 1);
                self.indent(depth);
                self.out.push_str("</rdf:li>\n");
            }
        }
    }
}

pub(crate) fn to_bytes(xmp: &Xmp) -> Vec<u8> {
    let prefixes = assign_prefixes(xmp);
    let mut w = Writer {
        out: String::new(),
        prefixes: &prefixes,
    };
    w.out
        .push_str("<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    w.out.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n");
    let _ = writeln!(w.out, "{INDENT}<rdf:RDF xmlns:rdf=\"{}\">", ns::RDF);
    let mut declarations: Vec<(&String, &String)> = prefixes.iter().map(|(u, p)| (p, u)).collect();
    declarations.sort();
    let _ = write!(w.out, "{INDENT}{INDENT}<rdf:Description rdf:about=\"\"");
    for (prefix, uri) in declarations {
        let _ = write!(w.out, "\n{INDENT}{INDENT}{INDENT}{INDENT}xmlns:{prefix}=\"");
        escape_attribute(&mut w.out, uri);
        w.out.push('"');
    }
    w.out.push_str(">\n");
    for p in &xmp.properties {
        w.property(p, 3);
    }
    let _ = writeln!(w.out, "{INDENT}{INDENT}</rdf:Description>");
    let _ = writeln!(w.out, "{INDENT}</rdf:RDF>");
    w.out.push_str("</x:xmpmeta>\n<?xpacket end=\"w\"?>\n");
    w.out.into_bytes()
}
