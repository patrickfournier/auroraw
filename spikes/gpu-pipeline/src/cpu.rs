//! CPU reference: the same maths as the shaders, in f32, multi-threaded with rayon. Serves as
//! the accuracy reference and as the "separate CPU implementation" fallback candidate.

use crate::{Params, scene::Scene};
use rayon::prelude::*;

fn mirror(i: i32, n: i32) -> i32 {
    let mut r = i;
    if r < 0 {
        r = -r;
    }
    if r >= n {
        r = 2 * n - 2 - r;
    }
    r.clamp(0, n - 1)
}

#[inline]
fn raw(scene: &Scene, p: &Params, x: i32, y: i32) -> f32 {
    let ix = mirror(x, scene.width as i32) as usize;
    let iy = mirror(y, scene.height as i32) as usize;
    (scene.mosaic[iy * scene.width as usize + ix] as f32 - p.black) / (p.white - p.black)
}

fn demosaic_generic(scene: &Scene, p: &Params, table: &[u32], x: i32, y: i32) -> [f32; 3] {
    let colour = |ix: i32, iy: i32| table[((iy % 6) * 6 + ix % 6) as usize] as usize;
    let (mut sum, mut weight) = ([0.0f32; 3], [0.0f32; 3]);
    for dy in -3..=3i32 {
        for dx in -3..=3i32 {
            let mx = mirror(x + dx, scene.width as i32);
            let my = mirror(y + dy, scene.height as i32);
            let c = colour(mx, my);
            let w = 1.0 / (1.0 + (dx * dx + dy * dy) as f32);
            sum[c] += w * raw(scene, p, mx, my);
            weight[c] += w;
        }
    }
    let mut rgb = [sum[0] / weight[0].max(1e-6), sum[1] / weight[1].max(1e-6), sum[2] / weight[2].max(1e-6)];
    rgb[colour(x, y)] = raw(scene, p, x, y);
    [rgb[0].max(0.0) * p.wb[0], rgb[1].max(0.0) * p.wb[1], rgb[2].max(0.0) * p.wb[2]]
}

fn demosaic(scene: &Scene, p: &Params, x: i32, y: i32) -> [f32; 3] {
    if let Some(table) = &scene.cfa6 {
        return demosaic_generic(scene, p, table, x, y);
    }
    let c = raw(scene, p, x, y);
    let (n, s, w, e) = (raw(scene, p, x, y - 1), raw(scene, p, x, y + 1), raw(scene, p, x - 1, y), raw(scene, p, x + 1, y));
    let (nn, ss, ww, ee) = (raw(scene, p, x, y - 2), raw(scene, p, x, y + 2), raw(scene, p, x - 2, y), raw(scene, p, x + 2, y));
    let (nw, ne, sw, se) = (raw(scene, p, x - 1, y - 1), raw(scene, p, x + 1, y - 1), raw(scene, p, x - 1, y + 1), raw(scene, p, x + 1, y + 1));
    let g_at_rb = (4.0 * c + 2.0 * (n + s + e + w) - (nn + ss + ee + ww)) / 8.0;
    let diag = nw + ne + sw + se;
    let (px, py) = ((x as u32 ^ p.cfa_flip) & 1, (y as u32 ^ (p.cfa_flip >> 1)) & 1);
    let rgb = match (px, py) {
        (0, 0) => [c, g_at_rb, (6.0 * c + 2.0 * diag - 1.5 * (nn + ss + ee + ww)) / 8.0],
        (1, 1) => [(6.0 * c + 2.0 * diag - 1.5 * (nn + ss + ee + ww)) / 8.0, g_at_rb, c],
        (1, 0) => [
            (5.0 * c + 4.0 * (w + e) - diag - (ww + ee) + 0.5 * (nn + ss)) / 8.0,
            c,
            (5.0 * c + 4.0 * (n + s) - diag - (nn + ss) + 0.5 * (ww + ee)) / 8.0,
        ],
        _ => [
            (5.0 * c + 4.0 * (n + s) - diag - (nn + ss) + 0.5 * (ww + ee)) / 8.0,
            c,
            (5.0 * c + 4.0 * (w + e) - diag - (ww + ee) + 0.5 * (nn + ss)) / 8.0,
        ],
    };
    [rgb[0].max(0.0) * p.wb[0], rgb[1].max(0.0) * p.wb[1], rgb[2].max(0.0) * p.wb[2]]
}

fn aces(x: f32) -> f32 {
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

fn srgb_oetf(l: f32) -> f32 {
    if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 }
}

fn lut_apply(lut: &[[f32; 4]], n: u32, c: [f32; 3]) -> [f32; 3] {
    let m = (n - 1) as f32;
    let pos = [c[0].clamp(0.0, 1.0) * m, c[1].clamp(0.0, 1.0) * m, c[2].clamp(0.0, 1.0) * m];
    let base = [pos[0].floor().min(m - 1.0), pos[1].floor().min(m - 1.0), pos[2].floor().min(m - 1.0)];
    let f = [pos[0] - base[0], pos[1] - base[1], pos[2] - base[2]];
    let at = |i: u32, j: u32, k: u32| lut[(i + n * (j + n * k)) as usize];
    let (i, j, k) = (base[0] as u32, base[1] as u32, base[2] as u32);
    let mix = |a: [f32; 4], b: [f32; 4], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, 0.0];
    let c00 = mix(at(i, j, k), at(i + 1, j, k), f[0]);
    let c10 = mix(at(i, j + 1, k), at(i + 1, j + 1, k), f[0]);
    let c01 = mix(at(i, j, k + 1), at(i + 1, j, k + 1), f[0]);
    let c11 = mix(at(i, j + 1, k + 1), at(i + 1, j + 1, k + 1), f[0]);
    let a = mix(c00, c10, f[1]);
    let b = mix(c01, c11, f[1]);
    let r = mix(a, b, f[2]);
    [r[0], r[1], r[2]]
}

fn tone(scene: &Scene, p: &Params, cam: [f32; 3]) -> u32 {
    let dot = |row: [f32; 4], v: [f32; 3]| row[0] * v[0] + row[1] * v[1] + row[2] * v[2];
    let w = [dot(p.cam[0], cam), dot(p.cam[1], cam), dot(p.cam[2], cam)].map(|v| aces(v.max(0.0) * p.exposure));
    let d = [dot(p.disp[0], w), dot(p.disp[1], w), dot(p.disp[2], w)].map(|v| srgb_oetf(v.clamp(0.0, 1.0)));
    let e = lut_apply(&scene.lut, p.lut_size, d);
    let q = e.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32);
    q[0] | (q[1] << 8) | (q[2] << 16) | (255 << 24)
}

/// Renders the region (x0, y0, w, h) of the scene to packed RGBA8.
pub fn render_region(scene: &Scene, x0: u32, y0: u32, w: u32, h: u32) -> Vec<u32> {
    let p = &scene.params;
    let mut out = vec![0u32; (w * h) as usize];
    out.par_chunks_mut(w as usize).enumerate().for_each(|(row, line)| {
        for (col, px) in line.iter_mut().enumerate() {
            let cam = demosaic(scene, p, (x0 + col as u32) as i32, (y0 + row as u32) as i32);
            *px = tone(scene, p, cam);
        }
    });
    out
}

// ---- the heavy chain: non-local means, blurs, unsharp / local contrast / mask, then tone ----

fn box_blur(src: &[[f32; 3]], rw: usize, rh: usize, radius: i32, vertical: bool) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0f32; 3]; rw * rh];
    out.par_chunks_mut(rw).enumerate().for_each(|(y, line)| {
        for (x, px) in line.iter_mut().enumerate() {
            let mut acc = [0.0f32; 3];
            for i in -radius..=radius {
                let (sx, sy) = if vertical {
                    (x, (y as i32 + i).clamp(0, rh as i32 - 1) as usize)
                } else {
                    ((x as i32 + i).clamp(0, rw as i32 - 1) as usize, y)
                };
                let v = src[sy * rw + sx];
                acc = [acc[0] + v[0], acc[1] + v[1], acc[2] + v[2]];
            }
            let n = (2 * radius + 1) as f32;
            *px = [acc[0] / n, acc[1] / n, acc[2] / n];
        }
    });
    out
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Renders the kept region (x0, y0, w, h), developed with a halo around it, through the whole
/// heavy chain. The same maths as the shaders.
pub fn chain_region(scene: &Scene, x0: u32, y0: u32, w: u32, h: u32, halo: u32) -> Vec<u32> {
    let p = &scene.params;
    let (rx, ry) = ((x0 - halo) as i32, (y0 - halo) as i32);
    let (rw, rh) = ((w + 2 * halo) as usize, (h + 2 * halo) as usize);
    let a: Vec<[f32; 3]> = (0..rw * rh).into_par_iter().map(|i| demosaic(scene, p, rx + (i % rw) as i32, ry + (i / rw) as i32)).collect();

    let (radius, patch) = (p.op[0][2] as i32, p.op[0][3] as i32);
    let h2 = (p.op[0][0] * p.op[0][0]).max(1e-8);
    let norm = 1.0 / (((2 * patch + 1) * (2 * patch + 1) * 3) as f32);
    let at = |x: i32, y: i32| a[(y.clamp(0, rh as i32 - 1) as usize) * rw + x.clamp(0, rw as i32 - 1) as usize];
    let den: Vec<[f32; 3]> = (0..rw * rh)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % rw) as i32, (i / rw) as i32);
            let (mut acc, mut wsum) = ([0.0f32; 3], 0.0f32);
            for sy in -radius..=radius {
                for sx in -radius..=radius {
                    let mut d = 0.0f32;
                    for py in -patch..=patch {
                        for px in -patch..=patch {
                            let (u, v) = (at(x + px, y + py), at(x + sx + px, y + sy + py));
                            d += (u[0] - v[0]).powi(2) + (u[1] - v[1]).powi(2) + (u[2] - v[2]).powi(2);
                        }
                    }
                    let wgt = (-(d * norm) / h2).exp();
                    let s = at(x + sx, y + sy);
                    acc = [acc[0] + wgt * s[0], acc[1] + wgt * s[1], acc[2] + wgt * s[2]];
                    wsum += wgt;
                }
            }
            [acc[0] / wsum, acc[1] / wsum, acc[2] / wsum]
        })
        .collect();

    let small = box_blur(&box_blur(&den, rw, rh, p.op[1][1] as i32, false), rw, rh, p.op[1][1] as i32, true);
    let large = box_blur(&box_blur(&den, rw, rh, p.op[2][1] as i32, false), rw, rh, p.op[2][1] as i32, true);
    let comb: Vec<[f32; 3]> = (0..rw * rh)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % rw) as f32, (i / rw) as f32);
            let (uvx, uvy) = (x / rw as f32, y / rh as f32);
            let dist = ((uvx - p.op[3][0]).powi(2) + (uvy - p.op[3][1]).powi(2)).sqrt();
            let m = 1.0 - smoothstep(0.5, 1.0, dist / p.op[3][2].max(1e-4));
            let gain = (p.op[3][3] * m).exp2();
            let mut out = [0.0f32; 3];
            for c in 0..3 {
                let v = den[i][c] + p.op[1][0] * (den[i][c] - small[i][c]) + p.op[2][0] * (den[i][c] - large[i][c]);
                out[c] = v.max(0.0) * gain;
            }
            out
        })
        .collect();

    let mut out = vec![0u32; (w * h) as usize];
    out.par_chunks_mut(w as usize).enumerate().for_each(|(row, line)| {
        for (col, px) in line.iter_mut().enumerate() {
            *px = tone(scene, p, comb[(row + halo as usize) * rw + col + halo as usize]);
        }
    });
    out
}
