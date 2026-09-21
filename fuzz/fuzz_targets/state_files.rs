// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
//! The state file readers on arbitrary bytes: what is accepted is written back canonically and
//! read again.

use auroraw_format::state::{read_state, write_state, Collection, Loaded, Marker, Series, Sources, StateBody, Vocabulary};
use libfuzzer_sys::fuzz_target;

fn check<T: StateBody + PartialEq + std::fmt::Debug>(data: &[u8]) {
    if let Ok(Loaded::Current(value)) = read_state::<T>(data) {
        let bytes = write_state(&value);
        let back = read_state::<T>(&bytes).expect("what is written can be read").current().expect("the current schema");
        assert_eq!(back, value);
    }
}

fuzz_target!(|data: &[u8]| {
    check::<Marker>(data);
    check::<Vocabulary>(data);
    check::<Sources>(data);
    check::<Collection>(data);
    check::<Series>(data);
});
