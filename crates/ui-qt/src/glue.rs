// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust half of the C++ glue (`glue.cpp`), everything the interface needs that cxx-qt-lib does not
//! wrap: the `image://thumbs/<photo id>` provider, loading a translation, and the QtQuickTest runner.
//! The only module with `unsafe` besides the cxx-qt bridges (D-094).

use std::ffi::c_void;
use std::pin::Pin;
use std::str::FromStr;

use auroraw_types::PhotoId;
use cxx_qt_lib::QQmlApplicationEngine;

use crate::session;

unsafe extern "C" {
    fn auroraw_setup_engine(engine: *mut c_void);
    fn auroraw_set_translation(data: *const u8, len: usize, object: *mut c_void);
    fn auroraw_shortcut_text(
        standard: i32,
        text: *const u8,
        len: usize,
        out: *mut u8,
        cap: usize,
    ) -> usize;
}

/// A shortcut as this platform writes it (`Ctrl+N`, `⌘N`): `standard` is a `QKeySequence::StandardKey`,
/// or negative to read `text` as a key sequence.
pub fn shortcut_text(standard: i32, text: &str) -> String {
    let mut out = [0u8; 64];
    // SAFETY: `text` and `out` are valid for the lengths given; the glue writes at most `out.len()`.
    let written = unsafe {
        auroraw_shortcut_text(
            standard,
            text.as_ptr(),
            text.len(),
            out.as_mut_ptr(),
            out.len(),
        )
    };
    String::from_utf8_lossy(&out[..written.min(out.len())]).into_owned()
}

/// Registers the thumbnail provider on `engine` and remembers it, so that a change of language can
/// retranslate what is on screen. The engine must outlive the application's event loop.
pub fn setup_engine(mut engine: Pin<&mut QQmlApplicationEngine>) {
    // SAFETY: the pointer is only handed to the glue, which keeps it for the application's life; the
    // engine is not moved (it is a C++ object behind a pin).
    unsafe {
        let raw = engine.as_mut().get_unchecked_mut() as *mut QQmlApplicationEngine;
        auroraw_setup_engine(raw as *mut c_void)
    }
}

/// Installs the translation in `qm` before any screen exists (nothing to retranslate).
pub fn install_translation(qm: Option<&'static [u8]>) {
    // SAFETY: a null object is allowed.
    unsafe { set_translation(qm, std::ptr::null_mut()) }
}

/// Installs the translation in `qm` (compiled `.qm` bytes, `'static` because Qt reads them in place)
/// in place of the previous one; `None` goes back to the source texts. When `object` is given (a
/// QObject made by QML, such as the launcher) its engine retranslates what is on screen.
///
/// # Safety
/// `object` is null or points at a live QObject.
pub unsafe fn set_translation(qm: Option<&'static [u8]>, object: *mut c_void) {
    let (data, len) = qm.map_or((std::ptr::null(), 0), |bytes| (bytes.as_ptr(), bytes.len()));
    // SAFETY: the bytes are `'static`, a null pointer with a length of 0 means "none", and the
    // caller guarantees `object`.
    unsafe { auroraw_set_translation(data, len, object) }
}

/// The thumbnail of the photo named `id` (UTF-8, `len` bytes), as a JPEG the caller must give back
/// to `auroraw_free_bytes`; null when there is none.
///
/// # Safety
/// `id` points at `len` readable bytes and `out_len` at a writable `usize`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn auroraw_thumbnail_jpeg(
    id: *const u8,
    len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    // SAFETY: the caller guarantees `id` and `len`.
    let name = unsafe { std::slice::from_raw_parts(id, len) };
    let bytes = std::str::from_utf8(name)
        .ok()
        .and_then(|text| PhotoId::from_str(text).ok())
        .and_then(|id| session::current()?.thumbs.jpeg(id));
    match bytes {
        Some(bytes) => {
            let boxed = bytes.into_boxed_slice();
            // SAFETY: the caller guarantees `out_len`.
            unsafe { *out_len = boxed.len() };
            Box::into_raw(boxed) as *mut u8
        }
        None => std::ptr::null_mut(),
    }
}

/// Gives back the bytes `auroraw_thumbnail_jpeg` returned.
///
/// # Safety
/// `bytes` and `len` are exactly what `auroraw_thumbnail_jpeg` returned, and are given back once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn auroraw_free_bytes(bytes: *mut u8, len: usize) {
    // SAFETY: the caller guarantees the pair came from `auroraw_thumbnail_jpeg`.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(bytes, len)) });
}

#[cfg(feature = "quicktest")]
mod quicktest {
    use std::ffi::{CString, c_char, c_int};

    unsafe extern "C" {
        fn auroraw_quick_test(argc: c_int, argv: *mut *mut c_char, dir: *const c_char) -> c_int;
    }

    /// Runs the QtQuickTest suites named on the command line (`-input <file>`), the folder of the
    /// tests being `dir`. Returns the number of failures.
    pub fn run(dir: &str) -> i32 {
        let args: Vec<CString> = std::env::args()
            .map(|a| CString::new(a).expect("no NUL in an argument"))
            .collect();
        let dir = CString::new(dir).expect("no NUL in a path");
        let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut _).collect();
        argv.push(std::ptr::null_mut());
        // SAFETY: argv is a null-terminated array of live C strings; dir is a live C string.
        unsafe { auroraw_quick_test(args.len() as c_int, argv.as_mut_ptr(), dir.as_ptr()) }
    }
}

#[cfg(feature = "quicktest")]
pub use quicktest::run as quick_test;
