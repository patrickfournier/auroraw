// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust half of the C++ glue (`glue.cpp`), everything the interface needs that cxx-qt-lib does not
//! wrap: the `image://thumbs/<photo id>` provider, loading a translation, and the QtQuickTest runner.
//! The only module with `unsafe` besides the cxx-qt bridges (D-094).

use std::ffi::c_void;
use std::str::FromStr;

use auroraw_types::PhotoId;

use crate::session;

unsafe extern "C" {
    fn auroraw_install_thumbnails(object: *mut c_void);
    fn auroraw_set_translation(data: *const u8, len: usize, object: *mut c_void);
    fn auroraw_thumbnail_ready(token: u64, bytes: *const u8, len: usize);
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

/// Registers the thumbnail provider (`image://thumbs/<photo id>`) on the engine of `object`.
///
/// # Safety
/// `object` points at a live QObject made by QML.
pub unsafe fn install_thumbnails(object: *mut c_void) {
    // SAFETY: the caller guarantees `object`.
    unsafe { auroraw_install_thumbnails(object) }
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

/// How the collector answers the image provider's requests: the C++ side decodes the JPEG (on the
/// collector's thread, off the interface's) and hands the image to the request that waits for it.
pub fn thumbnail_deliverer() -> session::Deliver {
    std::sync::Arc::new(|token, jpeg| {
        let (bytes, len) = jpeg.map_or((std::ptr::null(), 0), |b| (b.as_ptr(), b.len()));
        // SAFETY: `bytes` and `len` describe `jpeg` (or nothing, null with 0), which outlives the call.
        unsafe { auroraw_thumbnail_ready(token, bytes, len) }
    })
}

/// A request of the image provider (`image://thumbs/<photo id>`, the id being `id`, UTF-8, `len`
/// bytes) for a thumbnail; the answer goes to `auroraw_thumbnail_ready` under `token`.
///
/// # Safety
/// `id` points at `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn auroraw_thumbnail_request(id: *const u8, len: usize, token: u64) {
    // SAFETY: the caller guarantees `id` and `len`.
    let name = unsafe { std::slice::from_raw_parts(id, len) };
    let asked = std::str::from_utf8(name)
        .ok()
        .and_then(|text| PhotoId::from_str(text).ok())
        .zip(session::current());
    match asked {
        Some((id, session)) => session.thumbs.request(id, token),
        // SAFETY: no bytes (null, 0) is allowed.
        None => unsafe { auroraw_thumbnail_ready(token, std::ptr::null(), 0) },
    }
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
