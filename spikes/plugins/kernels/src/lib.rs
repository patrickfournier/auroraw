//! The image kernels shared by the plugins (compiled to WebAssembly) and by the host (compiled
//! natively, as the baseline). RGBA, f32, four values per pixel.

/// A display-transform-like operation: gamma with `powf` on every channel and a vignette that
/// needs a square root. Compute-bound.
pub fn tone_heavy(px: &mut [f32], w: usize, h: usize, strength: f32) {
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let norm = 1.0 / (cx * cx + cy * cy).sqrt();
    for y in 0..h {
        let dy = y as f32 - cy;
        for x in 0..w {
            let dx = x as f32 - cx;
            let v = 1.0 - strength * ((dx * dx + dy * dy).sqrt() * norm).powi(2);
            let i = (y * w + x) * 4;
            for c in 0..3 {
                px[i + c] = px[i + c].max(0.0).powf(0.4545) * v;
            }
        }
    }
}

/// A 5x5 box convolution with clamped edges. Neighbourhood access.
pub fn conv5(src: &[f32], dst: &mut [f32], w: usize, h: usize) {
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            for dy in -2i32..=2 {
                let yy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                for dx in -2i32..=2 {
                    let xx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                    let i = (yy * w + xx) * 4;
                    for c in 0..4 {
                        acc[c] += src[i + c];
                    }
                }
            }
            let o = (y * w + x) * 4;
            for c in 0..4 {
                dst[o + c] = acc[c] * (1.0 / 25.0);
            }
        }
    }
}

/// A saturation change in linear RGB, the CPU twin of the GPU plugin's shader.
pub fn saturation(px: &mut [f32], amount: f32) {
    for p in px.chunks_exact_mut(4) {
        let l = 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
        for c in 0..3 {
            p[c] = (l + (p[c] - l) * amount).max(0.0);
        }
    }
}

/// A pass over the memory, with almost no arithmetic: the cost of touching the data.
pub fn checksum(px: &[f32]) -> f32 {
    let mut s = 0.0f32;
    for v in px.iter().step_by(16) {
        s += *v;
    }
    s
}
