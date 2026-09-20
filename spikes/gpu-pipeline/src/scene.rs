//! A scene: a Bayer mosaic and the numbers needed to develop it. Synthetic for now; a real RAW
//! decoder plugs in here later.

use crate::{LUT_SIZE, Params};
use rayon::prelude::*;

pub struct Scene {
    pub width: u32,
    pub height: u32,
    /// RGGB mosaic, one 16-bit sample per pixel.
    pub mosaic: Vec<u16>,
    pub params: Params,
    pub lut: Vec<[f32; 4]>,
    /// Where the data came from, for reports.
    pub label: String,
    /// A 6x6 colour table (0 red, 1 green, 2 blue) selecting the generic demosaicing pass.
    pub cfa6: Option<Vec<u32>>,
}

fn hash(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

/// A smooth colour field with fine texture and noise, as scene-linear RGB.
fn signal(x: u32, y: u32, w: u32, h: u32) -> [f32; 3] {
    let u = x as f32 / w as f32;
    let v = y as f32 / h as f32;
    let rings = 0.5 + 0.5 * ((u - 0.5).hypot(v - 0.5) * 40.0).sin();
    let fine = 0.5 + 0.5 * ((x as f32) * 0.9).sin() * ((y as f32) * 0.7).cos();
    let noise = hash(x, y) - 0.5;
    let base = 0.05 + 0.6 * u * v;
    [
        (base + 0.25 * rings + 0.05 * fine + 0.02 * noise).max(0.0),
        (base * 0.9 + 0.2 * (1.0 - rings) * u + 0.05 * fine + 0.02 * noise).max(0.0),
        (base * 0.6 + 0.3 * (1.0 - v) + 0.04 * fine + 0.02 * noise).max(0.0),
    ]
}

pub fn synthetic(width: u32, height: u32) -> Scene {
    let (black, white) = (512.0f32, 16383.0f32);
    let wb = [2.0f32, 1.0, 1.6];
    let mut mosaic = vec![0u16; (width * height) as usize];
    mosaic.par_chunks_mut(width as usize).enumerate().for_each(|(y, row)| {
        let y = y as u32;
        for (x, px) in row.iter_mut().enumerate() {
            let x = x as u32;
            let s = signal(x, y, width, height);
            // The sensor sees the scene through the inverse of the white balance.
            let v = match (x & 1, y & 1) {
                (0, 0) => s[0] / wb[0],
                (1, 1) => s[2] / wb[2],
                _ => s[1] / wb[1],
            };
            *px = (black + v.clamp(0.0, 1.0) * (white - black)) as u16;
        }
    });
    let params = Params {
        width,
        height,
        tile_x: 0,
        tile_y: 0,
        tile_w: width,
        tile_h: height,
        black,
        white,
        wb: [wb[0], wb[1], wb[2], 1.0],
        cam: [[1.6, -0.4, -0.2, 0.0], [-0.3, 1.5, -0.2, 0.0], [0.0, -0.4, 1.4, 0.0]],
        // Rec.2020 (linear) to sRGB primaries (linear).
        disp: [[1.6605, -0.5876, -0.0728, 0.0], [-0.1246, 1.1329, -0.0083, 0.0], [-0.0182, -0.1006, 1.1187, 0.0]],
        exposure: 1.2,
        lut_size: LUT_SIZE,
        in_x0: 0,
        in_y0: 0,
        in_stride: width,
        out_x0: 0,
        out_y0: 0,
        out_stride: width,
        src_w: width,
        src_h: height,
        factor: 1,
        cfa_flip: 0,
    };
    Scene { width, height, mosaic, params, lut: make_lut(LUT_SIZE), label: "synthetic".into(), cfa6: None }
}

/// A 3D LUT that stands in for an ICC display profile: near identity with a little cross-talk.
pub fn make_lut(n: u32) -> Vec<[f32; 4]> {
    let mut lut = Vec::with_capacity((n * n * n) as usize);
    let m = (n - 1) as f32;
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let (r, g, b) = (i as f32 / m, j as f32 / m, k as f32 / m);
                lut.push([
                    (r + 0.02 * (g - b)).clamp(0.0, 1.0),
                    (g + 0.02 * (b - r)).clamp(0.0, 1.0),
                    (b + 0.02 * (r - g)).clamp(0.0, 1.0),
                    1.0,
                ]);
            }
        }
    }
    lut
}
