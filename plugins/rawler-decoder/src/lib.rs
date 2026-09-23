// SPDX-License-Identifier: GPL-3.0-or-later
//! The `rawler` RAW decoder, compiled to `wasm32-wasip1` and run through `auroraw-plugin-host`'s
//! sandbox (architecture §8.1, "Import"; spike 4). This plugin never touches the file system,
//! the network, or the clock: the host hands it a file's bytes and reads a decoded mosaic back
//! through the hand-made C interface `auroraw-plugin-api::Decoder` and `auroraw-plugin-host`
//! agree on (architecture §8.2b): `alloc` a buffer, write into it, call, read the result.
//!
//! `import`'s `out` parameter is a 28-byte buffer the host wrote (`alloc(28)`), filled with seven
//! little-endian `u32` values: width, height, components per pixel, the samples' address, their
//! count, and the black and white level as `f32::to_bits` (so the host reads them back exactly,
//! not truncated to an integer).

use rawler::RawImageData;
use rawler::decoders::RawDecodeParams;
use rawler::rawsource::RawSource;

/// Reserves `n` bytes in this instance's own memory and returns their address, so the host never
/// has to guess at an address inside this sandbox.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(n);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

/// What the last successful [`import`] kept, so its samples stay valid for the host to read after
/// the call returns; freed by [`release`].
static mut HELD: Vec<u16> = Vec::new();

/// Decodes the `len` bytes at `ptr` (a whole RAW file, copied in by the host) to its sensor
/// mosaic, writing width, height, components per pixel, the samples' address and count, and the
/// black and white level (as `f32` bits) to the seven `u32` slots at `out`. Returns `0` on
/// success; a negative code (not part of this plugin's contract beyond "failure") otherwise.
///
/// # Safety
///
/// `ptr` must be valid for `len` reads and `out` for 7 writes of `u32`: true of any address this
/// same instance's own [`alloc`] returned, which is the only address the host (`auroraw-plugin-host`)
/// ever passes here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn import(ptr: *const u8, len: u32, out: *mut u32) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let source = RawSource::new_from_slice(bytes);
    let image = match rawler::decode(&source, &RawDecodeParams::default()) {
        Ok(image) => image,
        Err(_) => return -1,
    };
    let RawImageData::Integer(samples) = image.data else {
        return -2;
    };
    let black = image.blacklevel.as_bayer_array().iter().sum::<f32>() / 4.0;
    let white = image.whitelevel.as_bayer_array().iter().sum::<f32>() / 4.0;
    #[allow(static_mut_refs)]
    unsafe {
        HELD = samples;
        let header = std::slice::from_raw_parts_mut(out, 7);
        header[0] = image.width as u32;
        header[1] = image.height as u32;
        header[2] = image.cpp as u32;
        header[3] = HELD.as_ptr() as u32;
        header[4] = HELD.len() as u32;
        header[5] = black.to_bits();
        header[6] = white.to_bits();
    }
    0
}

/// Frees what [`import`] kept. The host calls this once it has read the samples back; the host
/// must not read them afterwards.
#[unsafe(no_mangle)]
pub extern "C" fn release() {
    #[allow(static_mut_refs)]
    unsafe {
        HELD = Vec::new();
    }
}
