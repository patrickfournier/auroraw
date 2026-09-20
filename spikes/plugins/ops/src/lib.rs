//! An operation plugin: image kernels behind a small C ABI. The host allocates buffers in the
//! plugin's memory, writes the pixels, calls, and reads the result back.
use kernels::*;

#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

#[unsafe(no_mangle)]
pub extern "C" fn dealloc(p: *mut u8, n: usize) {
    unsafe { drop(Vec::from_raw_parts(p, 0, n)) }
}

#[unsafe(no_mangle)]
pub extern "C" fn noop(_w: u32, _h: u32) -> u32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn op_tone_heavy(px: *mut f32, w: u32, h: u32, strength: f32) {
    let s = unsafe { std::slice::from_raw_parts_mut(px, (w * h * 4) as usize) };
    tone_heavy(s, w as usize, h as usize, strength);
}

#[unsafe(no_mangle)]
pub extern "C" fn op_conv5(src: *const f32, dst: *mut f32, w: u32, h: u32) {
    let n = (w * h * 4) as usize;
    let (s, d) = unsafe { (std::slice::from_raw_parts(src, n), std::slice::from_raw_parts_mut(dst, n)) };
    conv5(s, d, w as usize, h as usize);
}

#[unsafe(no_mangle)]
pub extern "C" fn op_saturation(px: *mut f32, w: u32, h: u32, amount: f32) {
    let s = unsafe { std::slice::from_raw_parts_mut(px, (w * h * 4) as usize) };
    saturation(s, amount);
}

#[unsafe(no_mangle)]
pub extern "C" fn op_checksum(px: *const f32, w: u32, h: u32) -> f32 {
    checksum(unsafe { std::slice::from_raw_parts(px, (w * h * 4) as usize) })
}
