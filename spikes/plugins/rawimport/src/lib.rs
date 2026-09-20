//! An import plugin: decodes a RAW file that the host hands over as bytes. The plugin never
//! touches the file system.
use rawler::{RawImageData, decoders::RawDecodeParams, rawsource::RawSource};

#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

static mut HELD: Vec<u16> = Vec::new();

/// Decodes `len` bytes at `ptr`. On success fills `out` with seven values: width, height,
/// components per pixel, the address of the samples, their count, black level, white level.
#[unsafe(no_mangle)]
pub extern "C" fn import(ptr: *const u8, len: u32, out: *mut u32) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let src = RawSource::new_from_slice(bytes);
    let img = match rawler::decode(&src, &RawDecodeParams::default()) {
        Ok(i) => i,
        Err(_) => return -1,
    };
    let RawImageData::Integer(data) = img.data else { return -2 };
    let black = img.blacklevel.as_bayer_array().iter().sum::<f32>() / 4.0;
    let white = img.whitelevel.as_bayer_array().iter().sum::<f32>() / 4.0;
    #[allow(static_mut_refs)]
    unsafe {
        HELD = data;
        let o = std::slice::from_raw_parts_mut(out, 7);
        o[0] = img.width as u32; o[1] = img.height as u32; o[2] = img.cpp as u32;
        o[3] = HELD.as_ptr() as u32; o[4] = HELD.len() as u32; o[5] = black as u32; o[6] = white as u32;
    }
    0
}

/// Frees what `import` kept.
#[unsafe(no_mangle)]
pub extern "C" fn release() {
    #[allow(static_mut_refs)]
    unsafe { HELD = Vec::new(); }
}
