//! Spike 1: a GPU pipeline for RAW development on wgpu, with a CPU reference.
//!
//! Throwaway code that answers one question (see docs/technical-spikes.md): can one Rust
//! pipeline on wgpu produce a colour-managed image fast enough, with matching results across
//! back ends, and with a CPU fallback? It is not part of the product.

pub mod cpu;
pub mod gpu;
pub mod scene;

/// Uniform block shared by every pass. Must match `shaders/common.wgsl` field for field.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Params {
    pub width: u32,
    pub height: u32,
    pub tile_x: u32,
    pub tile_y: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub black: f32,
    pub white: f32,
    pub wb: [f32; 4],
    pub cam: [[f32; 4]; 3],
    pub disp: [[f32; 4]; 3],
    pub exposure: f32,
    pub lut_size: u32,
    pub in_x0: u32,
    pub in_y0: u32,
    pub in_stride: u32,
    pub out_x0: u32,
    pub out_y0: u32,
    pub out_stride: u32,
    pub src_w: u32,
    pub src_h: u32,
    pub factor: u32,
    pub pad: u32,
}

pub const LUT_SIZE: u32 = 33;

/// Differences between two 8-bit RGBA images, per channel level.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Diff {
    pub max_level: u32,
    pub fraction_over_one_level: f64,
    pub mean_level: f64,
}

pub fn diff(a: &[u32], b: &[u32]) -> Diff {
    assert_eq!(a.len(), b.len());
    let mut max = 0u32;
    let mut over = 0u64;
    let mut sum = 0u64;
    for (&x, &y) in a.iter().zip(b) {
        for shift in [0, 8, 16] {
            let d = (((x >> shift) & 255) as i32 - ((y >> shift) & 255) as i32).unsigned_abs();
            max = max.max(d);
            sum += d as u64;
            if d > 1 {
                over += 1;
            }
        }
    }
    let channels = (a.len() * 3) as f64;
    Diff { max_level: max, fraction_over_one_level: over as f64 / channels, mean_level: sum as f64 / channels }
}
