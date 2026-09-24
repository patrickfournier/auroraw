// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust half of the thumbnail image provider: C++ (`thumbs.cpp`) asks for a photo's JPEG by
//! id and registers the provider on the QML engine.

use std::ffi::c_void;
use std::str::FromStr;

use auroraw_types::PhotoId;

use crate::session;

unsafe extern "C" {
    fn spike_register_thumbs(engine: *mut c_void);
    fn spike_install_translation(path: *const std::ffi::c_char) -> bool;
    fn spike_quick_test(
        argc: std::ffi::c_int,
        argv: *mut *mut std::ffi::c_char,
        dir: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
}

/// Runs `tests/tst_*.qml` under QtQuickTest's runner (`--quicktest <dir> [runner options]`).
pub fn quick_test() {
    use std::ffi::CString;
    let args: Vec<CString> = std::env::args()
        .filter(|a| a != "--quicktest")
        .map(|a| CString::new(a).unwrap())
        .collect();
    let dir = CString::new(std::env::var("SPIKE_TESTS").expect("SPIKE_TESTS is the tests folder"))
        .unwrap();
    let mut argv: Vec<*mut std::ffi::c_char> = args.iter().map(|a| a.as_ptr() as *mut _).collect();
    argv.push(std::ptr::null_mut());
    // SAFETY: argv is a null-terminated array of live C strings; dir is a live C string.
    let status = unsafe { spike_quick_test(args.len() as i32, argv.as_mut_ptr(), dir.as_ptr()) };
    std::process::exit(status);
}

/// Registers the `image://thumbs/<photo id>` provider on `engine` (a `QQmlApplicationEngine`).
pub fn register(engine: *mut c_void) {
    // SAFETY: `engine` is a live QQmlApplicationEngine for the duration of the call.
    unsafe { spike_register_thumbs(engine) }
}

/// The thumbnail of the photo named `id` (UTF-8, `len` bytes), as a JPEG the caller must give back
/// to `spike_free_bytes`; null when there is none.
///
/// # Safety
/// `id` points at `len` readable bytes and `out_len` at a writable `usize`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spike_thumbnail_jpeg(
    id: *const u8,
    len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let name = unsafe { std::slice::from_raw_parts(id, len) };
    let bytes = std::str::from_utf8(name)
        .ok()
        .and_then(|text| PhotoId::from_str(text).ok())
        .and_then(|id| session::current()?.thumbs.jpeg(id));
    match bytes {
        Some(bytes) => {
            let boxed = bytes.into_boxed_slice();
            unsafe { *out_len = boxed.len() };
            Box::into_raw(boxed) as *mut u8
        }
        None => std::ptr::null_mut(),
    }
}

/// Gives back the bytes `spike_thumbnail_jpeg` returned.
///
/// # Safety
/// `bytes` and `len` are exactly what `spike_thumbnail_jpeg` returned, and are given back once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spike_free_bytes(bytes: *mut u8, len: usize) {
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(bytes, len)) });
}

/// Installs the translation file at `path` (a `.qm`); whether it loaded.
pub fn install_translation(path: &str) -> bool {
    let path = std::ffi::CString::new(path).unwrap();
    // SAFETY: `path` is a live C string, and the application object exists.
    unsafe { spike_install_translation(path.as_ptr()) }
}
