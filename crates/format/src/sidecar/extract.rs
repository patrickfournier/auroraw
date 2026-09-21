// SPDX-License-Identifier: GPL-3.0-or-later
//! Helpers that take known properties out of a list, leaving the rest in place.

use std::str::FromStr;

use crate::xmp::{ArrayKind, Property};

fn position(
    props: &[Property],
    ns: &str,
    name: &str,
    ok: impl Fn(&Property) -> bool,
) -> Option<usize> {
    props.iter().position(|p| p.is(ns, name) && ok(p))
}

/// A simple text property. A property of another shape is left where it is.
pub(crate) fn text(props: &mut Vec<Property>, ns: &str, name: &str) -> Option<String> {
    let i = position(props, ns, name, |p| p.as_text().is_some())?;
    props.remove(i).as_text().map(str::to_string)
}

/// A language alternative (or a simple text): its default text.
pub(crate) fn lang_text(props: &mut Vec<Property>, ns: &str, name: &str) -> Option<String> {
    let i = position(props, ns, name, |p| p.as_lang_text().is_some())?;
    props.remove(i).as_lang_text().map(str::to_string)
}

/// An array of simple texts.
pub(crate) fn texts(props: &mut Vec<Property>, ns: &str, name: &str) -> Option<Vec<String>> {
    let i = position(props, ns, name, |p| p.as_texts().is_some())?;
    Some(
        props
            .remove(i)
            .as_texts()?
            .into_iter()
            .map(str::to_string)
            .collect(),
    )
}

/// A simple text that parses; one that does not is left where it is (and so is kept).
pub(crate) fn parsed<T: FromStr>(props: &mut Vec<Property>, ns: &str, name: &str) -> Option<T> {
    let i = position(props, ns, name, |p| {
        p.as_text().and_then(|t| t.parse::<T>().ok()).is_some()
    })?;
    props.remove(i).as_text().and_then(|t| t.parse().ok())
}

/// An array of texts that all parse; otherwise left where it is.
pub(crate) fn parsed_array<T: FromStr>(
    props: &mut Vec<Property>,
    ns: &str,
    name: &str,
) -> Option<Vec<T>> {
    let i = position(props, ns, name, |p| {
        p.as_texts()
            .is_some_and(|v| v.iter().all(|t| t.parse::<T>().is_ok()))
    })?;
    let p = props.remove(i);
    Some(
        p.as_texts()?
            .into_iter()
            .filter_map(|t| t.parse().ok())
            .collect(),
    )
}

/// A structure property, removed.
pub(crate) fn structure(props: &mut Vec<Property>, ns: &str, name: &str) -> Option<Vec<Property>> {
    let i = position(props, ns, name, |p| p.as_fields().is_some())?;
    match props.remove(i).value {
        crate::xmp::Value::Struct(f) => Some(f),
        _ => None,
    }
}

/// The items of a sequence of structures, or none if the property is anything else.
pub(crate) fn struct_items(
    props: &mut Vec<Property>,
    ns: &str,
    name: &str,
) -> Option<Vec<Vec<Property>>> {
    let i = position(props, ns, name, |p| match &p.value {
        crate::xmp::Value::Array(_, items) => items
            .iter()
            .all(|it| matches!(it.value, crate::xmp::Value::Struct(_))),
        _ => false,
    })?;
    match props.remove(i).value {
        crate::xmp::Value::Array(_, items) => Some(
            items
                .into_iter()
                .filter_map(|it| match it.value {
                    crate::xmp::Value::Struct(f) => Some(f),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

pub(crate) fn opt_text(ns: &str, name: &str, value: &Option<String>) -> Option<Property> {
    value.as_ref().map(|v| Property::text(ns, name, v.clone()))
}

pub(crate) fn opt_lang(ns: &str, name: &str, value: &Option<String>) -> Option<Property> {
    value
        .as_ref()
        .map(|v| Property::lang_alt(ns, name, v.clone()))
}

pub(crate) fn opt_array(
    ns: &str,
    name: &str,
    kind: ArrayKind,
    values: &[String],
) -> Option<Property> {
    (!values.is_empty()).then(|| Property::array(ns, name, kind, values.iter().cloned()))
}
