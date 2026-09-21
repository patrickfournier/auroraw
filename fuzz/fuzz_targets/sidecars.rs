// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
//! The photo and version sidecar readers on arbitrary bytes.

use auroraw_format::sidecar::{PhotoSidecar, VersionSidecar};
use auroraw_format::state::Loaded;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(Loaded::Current(photo)) = PhotoSidecar::from_bytes(data) {
        let bytes = photo.to_bytes();
        assert!(PhotoSidecar::from_bytes(&bytes).is_ok(), "what is written can be read");
    }
    if let Ok(Loaded::Current(version)) = VersionSidecar::from_bytes(data) {
        let bytes = version.to_bytes();
        assert!(VersionSidecar::from_bytes(&bytes).is_ok(), "what is written can be read");
    }
});
