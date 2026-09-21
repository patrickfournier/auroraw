// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
//! The XMP reader on arbitrary bytes: it must never panic, hang or grow without bound, and what
//! it accepts must be written and read back to the same model (design note 003 §9).

use auroraw_format::xmp::Xmp;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(xmp) = Xmp::from_bytes(data) {
        let bytes = xmp.to_bytes();
        let back = Xmp::from_bytes(&bytes).expect("what is written can be read");
        assert_eq!(back.properties.len(), xmp.properties.len());
        assert_eq!(back.to_bytes(), bytes, "the canonical form is stable");
    }
});
