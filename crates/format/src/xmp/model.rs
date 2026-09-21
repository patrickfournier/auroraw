// SPDX-License-Identifier: GPL-3.0-or-later
use super::read::{self, XmpError};
use super::write;

/// The kind of an XMP array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayKind {
    /// `rdf:Seq`: ordered.
    Seq,
    /// `rdf:Bag`: unordered.
    Bag,
    /// `rdf:Alt`: alternatives, typically language alternatives.
    Alt,
}

/// The value of a property.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Simple text.
    Text(String),
    /// A resource reference, `rdf:resource`.
    Resource(String),
    /// A structure: named fields.
    Struct(Vec<Property>),
    /// An array of items.
    Array(ArrayKind, Vec<Item>),
}

/// An item of an array.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// The language, for a language alternative.
    pub lang: Option<String>,
    /// The item's value.
    pub value: Value,
}

/// A property (or a structure field): a name in a namespace, and a value.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    /// The namespace URI.
    pub ns: String,
    /// The local name.
    pub name: String,
    /// The language of a simple text value, if it has one.
    pub lang: Option<String>,
    /// The value.
    pub value: Value,
}

impl Property {
    /// A simple text property.
    pub fn text(ns: &str, name: &str, text: impl Into<String>) -> Self {
        Self {
            ns: ns.into(),
            name: name.into(),
            lang: None,
            value: Value::Text(text.into()),
        }
    }

    /// An array property of simple text items.
    pub fn array(
        ns: &str,
        name: &str,
        kind: ArrayKind,
        items: impl IntoIterator<Item = String>,
    ) -> Self {
        let items = items
            .into_iter()
            .map(|t| Item {
                lang: None,
                value: Value::Text(t),
            })
            .collect();
        Self {
            ns: ns.into(),
            name: name.into(),
            lang: None,
            value: Value::Array(kind, items),
        }
    }

    /// A language alternative with a single `x-default` text.
    pub fn lang_alt(ns: &str, name: &str, text: impl Into<String>) -> Self {
        let item = Item {
            lang: Some("x-default".into()),
            value: Value::Text(text.into()),
        };
        Self {
            ns: ns.into(),
            name: name.into(),
            lang: None,
            value: Value::Array(ArrayKind::Alt, vec![item]),
        }
    }

    /// A structure property.
    pub fn structure(ns: &str, name: &str, fields: Vec<Property>) -> Self {
        Self {
            ns: ns.into(),
            name: name.into(),
            lang: None,
            value: Value::Struct(fields),
        }
    }

    /// The simple text, if this is a simple text property.
    pub fn as_text(&self) -> Option<&str> {
        match &self.value {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }

    /// The texts of the items of an array of simple values, or the single text of a language
    /// alternative (its `x-default` item, else its first).
    pub fn as_texts(&self) -> Option<Vec<&str>> {
        match &self.value {
            Value::Array(_, items) => items
                .iter()
                .map(|i| match &i.value {
                    Value::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    }

    /// The text of a language alternative: its `x-default` item, else its first.
    pub fn as_lang_text(&self) -> Option<&str> {
        match &self.value {
            Value::Array(_, items) => items
                .iter()
                .find(|i| i.lang.as_deref() == Some("x-default"))
                .or_else(|| items.first())
                .and_then(|i| match &i.value {
                    Value::Text(t) => Some(t.as_str()),
                    _ => None,
                }),
            Value::Text(t) => Some(t),
            _ => None,
        }
    }

    /// The fields of a structure.
    pub fn as_fields(&self) -> Option<&[Property]> {
        match &self.value {
            Value::Struct(f) => Some(f),
            _ => None,
        }
    }

    /// Whether this is the property `name` of the namespace `ns`.
    pub fn is(&self, ns: &str, name: &str) -> bool {
        self.ns == ns && self.name == name
    }
}

/// An XMP document as an ordered list of properties.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Xmp {
    /// The properties, in document order, across every `rdf:Description`.
    pub properties: Vec<Property>,
    /// Namespace prefixes seen when reading, as hints for namespaces this crate does not know.
    pub prefixes: Vec<(String, String)>,
}

impl Xmp {
    /// Reads an XMP document (a sidecar, or an XMP packet).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, XmpError> {
        read::parse(bytes)
    }

    /// The canonical bytes: one description, elements only, fixed indentation.
    pub fn to_bytes(&self) -> Vec<u8> {
        write::to_bytes(self)
    }

    /// The first property `name` of the namespace `ns`.
    pub fn get(&self, ns: &str, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.is(ns, name))
    }

    /// Removes and returns the first property `name` of the namespace `ns`.
    pub fn take(&mut self, ns: &str, name: &str) -> Option<Property> {
        let i = self.properties.iter().position(|p| p.is(ns, name))?;
        Some(self.properties.remove(i))
    }
}
