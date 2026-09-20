//! Loads a real RAW file with rawler and turns it into a `Scene`.
//!
//! Only 2x2 Bayer sensors are supported by the demosaicing pass; anything else is reported as
//! unsupported, so the benchmark can say which cameras still need work.

use crate::{LUT_SIZE, Params, scene::{Scene, make_lut}};
use anyhow::{Result, anyhow, bail};
use rawler::{RawImageData, imgop::xyz::Illuminant, rawimage::RawPhotometricInterpretation};
use std::path::Path;

fn inverse3(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let [[a, b, c], [d, e, f], [g, h, i]] = m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    let inv = 1.0 / det;
    [
        [(e * i - f * h) * inv, (c * h - b * i) * inv, (b * f - c * e) * inv],
        [(f * g - d * i) * inv, (a * i - c * g) * inv, (c * d - a * f) * inv],
        [(d * h - e * g) * inv, (b * g - a * h) * inv, (a * e - b * d) * inv],
    ]
}

fn mul3(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

/// Rec.2020 (linear, D65) to XYZ.
const REC2020_TO_XYZ: [[f32; 3]; 3] = [[0.636958, 0.144617, 0.168881], [0.262700, 0.677998, 0.059302], [0.0, 0.028073, 1.060985]];

pub fn load(path: &Path, force_generic: bool) -> Result<Scene> {
    let raw = rawler::decode_file(path).map_err(|e| anyhow!("decode: {e}"))?;
    let RawPhotometricInterpretation::Cfa(cfg) = &raw.photometric else { bail!("not a CFA image") };
    let bayer = cfg.cfa.width == 2 && cfg.cfa.height == 2;
    let sixes = cfg.cfa.width == 6 && cfg.cfa.height == 6;
    if !cfg.cfa.is_rgb() || !(bayer || sixes) {
        bail!("unsupported CFA {} ({}x{})", cfg.cfa.name, cfg.cfa.width, cfg.cfa.height);
    }
    let RawImageData::Integer(data) = &raw.data else { bail!("float RAW data") };

    // Crop to the recommended area, on even coordinates so the Bayer phase is easy to keep.
    let rect = raw.crop_area.or(raw.active_area);
    let (mut x0, mut y0, mut w, mut h) = match rect {
        Some(r) => (r.p.x, r.p.y, r.d.w, r.d.h),
        None => (0, 0, raw.width, raw.height),
    };
    x0 &= !1;
    y0 &= !1;
    w = (w & !1).min(raw.width - x0);
    h = (h & !1).min(raw.height - y0);

    // Bayer: which parity flips turn the RGGB pattern into this crop's pattern?
    let rggb = |x: usize, y: usize| [[0usize, 1], [1, 2]][y & 1][x & 1];
    let flip = if bayer {
        (0..4u32)
            .find(|&f| {
                let (fx, fy) = ((f & 1) as usize, (f >> 1) as usize);
                (0..2).all(|dy| (0..2).all(|dx| cfg.cfa.color_at(y0 + dy, x0 + dx) == rggb(dx ^ fx, dy ^ fy)))
            })
            .ok_or_else(|| anyhow!("CFA {} has no RGGB phase", cfg.cfa.name))?
    } else {
        0
    };
    // The generic pass takes the colour of each position in a 6x6 window at the crop's origin.
    let cfa6 = if sixes || force_generic {
        Some((0..36).map(|i| cfg.cfa.color_at(y0 + i / 6, x0 + i % 6) as u32).collect::<Vec<u32>>())
    } else {
        None
    };

    let mut mosaic = Vec::with_capacity(w * h);
    for y in 0..h {
        let start = (y0 + y) * raw.width + x0;
        mosaic.extend_from_slice(&data[start..start + w]);
    }

    let mean4 = |v: [f32; 4]| (v[0] + v[1] + v[2] + v[3]) / 4.0;
    let black = mean4(raw.blacklevel.as_bayer_array());
    let white = mean4(raw.whitelevel.as_bayer_array());

    let mut wb = raw.wb_coeffs;
    if !wb.iter().take(3).all(|v| v.is_finite() && *v > 0.0) {
        wb = raw.neutralwb();
    }
    let wb = [wb[0] / wb[1], 1.0, wb[2] / wb[1]];

    // Camera RGB -> Rec.2020: dcraw's method, from the camera's XYZ->camera matrix.
    // The camera's XYZ -> camera matrix: prefer the D65 one, else any other, else the legacy field.
    let flat = raw
        .color_matrix_find_first([Illuminant::D65, Illuminant::Daylight, Illuminant::D50, Illuminant::A])
        .map(|(_, m)| m)
        .or_else(|| raw.color_matrix.values().next().cloned());
    let xyz_to_cam = match flat {
        Some(m) if m.len() >= 9 => [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]],
        _ => [raw.xyz_to_cam[0], raw.xyz_to_cam[1], raw.xyz_to_cam[2]],
    };
    let cam_rgb = mul3(xyz_to_cam, REC2020_TO_XYZ);
    let mut norm = cam_rgb;
    for row in norm.iter_mut() {
        let sum: f32 = row.iter().sum();
        if sum.abs() > 1e-6 {
            row.iter_mut().for_each(|v| *v /= sum);
        }
    }
    let usable = norm.iter().all(|r| r.iter().all(|v| v.is_finite())) && xyz_to_cam.iter().flatten().any(|v| *v != 0.0);
    let cam_to_working = if usable { inverse3(norm) } else { [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] };

    // Exposure so that the average green sits around middle grey.
    let step = 97usize;
    let (mut sum, mut n) = (0.0f64, 0u64);
    for (i, v) in mosaic.iter().enumerate().step_by(step) {
        let (x, y) = (i % w, i / w);
        let green = match &cfa6 { Some(t) => t[(y % 6) * 6 + x % 6] == 1, None => (x ^ (flip as usize & 1)) & 1 != (y ^ (flip as usize >> 1)) & 1 };
        if green {
            sum += ((*v as f32 - black) / (white - black)).max(0.0) as f64;
            n += 1;
        }
    }
    let mean = (sum / n.max(1) as f64) as f32;
    let exposure = (0.18 / mean.max(1e-4)).clamp(0.5, 16.0);

    let params = Params {
        width: w as u32,
        height: h as u32,
        tile_x: 0,
        tile_y: 0,
        tile_w: w as u32,
        tile_h: h as u32,
        black,
        white,
        wb: [wb[0], wb[1], wb[2], 1.0],
        cam: [
            [cam_to_working[0][0], cam_to_working[0][1], cam_to_working[0][2], 0.0],
            [cam_to_working[1][0], cam_to_working[1][1], cam_to_working[1][2], 0.0],
            [cam_to_working[2][0], cam_to_working[2][1], cam_to_working[2][2], 0.0],
        ],
        disp: [[1.6605, -0.5876, -0.0728, 0.0], [-0.1246, 1.1329, -0.0083, 0.0], [-0.0182, -0.1006, 1.1187, 0.0]],
        exposure,
        lut_size: LUT_SIZE,
        in_x0: 0,
        in_y0: 0,
        in_stride: w as u32,
        out_x0: 0,
        out_y0: 0,
        out_stride: w as u32,
        src_w: w as u32,
        src_h: h as u32,
        factor: 1,
        cfa_flip: flip,
        op: crate::DEFAULT_OPS,
        blur: [0; 4],
    };
    let label = format!("{} {} ({}, {}x{}, black {:.0}, white {:.0}, matrix {})", raw.clean_make, raw.clean_model, cfg.cfa.name, w, h, black, white, if usable { "camera" } else { "MISSING" });
    Ok(Scene { width: w as u32, height: h as u32, mosaic, params, lut: make_lut(LUT_SIZE), label, cfa6 })
}
