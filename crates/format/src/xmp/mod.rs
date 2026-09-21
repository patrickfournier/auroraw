// SPDX-License-Identifier: GPL-3.0-or-later
//! XMP: a general reader and a strict canonical writer (design note 003 §3 and §4.1).
//!
//! The reader accepts the equivalent forms other software writes (properties as attributes,
//! several `rdf:Description` blocks, `rdf:parseType="Resource"`, nested descriptions) into one
//! model: a flat, ordered list of properties. **Nothing is dropped**: a property this crate does
//! not interpret is just a property, and is written back. A construct it cannot represent is an
//! error, so a file is never silently damaged.
//!
//! The writer emits one `rdf:Description`, every property as an element, arrays as `rdf:Seq`,
//! `rdf:Bag` or `rdf:Alt`, two-space indentation and LF: the same model always gives the same bytes.

mod model;
mod names;
mod read;
mod write;

pub use model::{ArrayKind, Item, Property, Value, Xmp};
pub use names::ns;
pub use read::XmpError;
