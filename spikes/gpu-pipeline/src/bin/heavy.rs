//! Spike 1, second round: a heavier pipeline (non-local means denoising, unsharp mask, local
//! contrast, a radial exposure mask) and where it breaks the 50 ms budget.
//!
//! usage: heavy --adapter <words> [--file path] [--out file.json]

use anyhow::Result;
use gpu_pipeline::{cpu, diff, gpu::{Gpu, Renderer, STAGES, Spec, Stage}, raw, scene};
use serde_json::json;
use std::path::Path;

const HALO: u32 = 40;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn spec_for(vx: u32, vy: u32, vw: u32, vh: u32) -> Spec {
    Spec { x0: vx - HALO, y0: vy - HALO, rw: vw + 2 * HALO, rh: vh + 2 * HALO, halo_x: HALO, halo_y: HALO, inner_w: vw, inner_h: vh, out_x0: 0, out_y0: 0, out_stride: vw }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let adapter = get("--adapter").unwrap_or_else(|| "vulkan".into());
    let gpu = Gpu::select(&adapter)?;
    let scene = match get("--file") {
        Some(f) if f != "synthetic" => raw::load(Path::new(&f), false)?,
        _ => scene::synthetic(6000, 4000),
    };
    let (w, h) = (scene.width, scene.height);
    eprintln!("adapter: {} [{:?}]\nscene: {} ({:.1} MP)", gpu.info.name, gpu.info.backend, scene.label, (w * h) as f64 / 1e6);
    let r = Renderer::new(&gpu, &scene)?;
    eprintln!("upload of the mosaic and LUT: {:.1} ms", r.upload_ms);
    let base = r.scene_params;

    // 1. Are the shaders right? A small region through the whole chain, against the CPU.
    let (aw, ah) = (256u32, 256u32);
    let (ax, ay) = ((w - aw) / 2, (h - ah) / 2);
    let spec = Spec { x0: ax - HALO, y0: ay - HALO, rw: aw + 2 * HALO, rh: ah + 2 * HALO, halo_x: HALO, halo_y: HALO, inner_w: aw, inner_h: ah, out_x0: 0, out_y0: 0, out_stride: aw };
    let chain = r.chain(spec.rw, spec.rh, (aw * ah) as u64);
    r.chain_run(&chain, &spec, &base, Stage::Demosaic)?;
    let gpu_px = r.read(&chain.out, (aw * ah) as u64)?;
    let cpu_px = cpu::chain_region(&scene, ax, ay, aw, ah, HALO);
    let accuracy = diff(&gpu_px, &cpu_px);
    let mean_byte = gpu_px.iter().map(|p| ((p & 255) + ((p >> 8) & 255) + ((p >> 16) & 255)) as f64 / 3.0).sum::<f64>() / gpu_px.len() as f64;
    let distinct = { let mut v = gpu_px.clone(); v.sort_unstable(); v.dedup(); v.len() };
    eprintln!("chain, 256x256 region, GPU vs CPU reference: {accuracy:?}; mean level {mean_byte:.1}, {distinct} distinct colours");
    drop(chain);

    // 2. The 100% view: the largest of 2560x1440, 1920x1080 and 1280x720 that fits in GPU memory.
    let mut picked = None;
    for (sw, sh) in [(2560u32, 1440u32), (1920, 1080), (1280, 720), (640, 360)] {
        let (vw, vh) = (sw.min(w - 2 * HALO), sh.min(h - 2 * HALO));
        let (vx, vy) = ((w - vw) / 2, (h - vh) / 2);
        let spec = spec_for(vx, vy, vw, vh);
        if let Some(c) = r.try_chain(spec.rw, spec.rh, (vw * vh) as u64) {
            picked = Some((c, spec, vw, vh));
            break;
        }
        eprintln!("{sw}x{sh} does not fit in GPU memory, trying smaller");
    }
    let (chain, spec, vw, vh) = picked.expect("not even 640x360 fits in GPU memory");
    eprintln!("100% view size used: {vw}x{vh} (+ {HALO} px halo)");
    for _ in 0..3 {
        r.chain_run(&chain, &spec, &base, Stage::Demosaic)?;
    }
    let mut stages = serde_json::Map::new();
    for st in STAGES {
        let t = median((0..10).map(|_| r.chain_stage(&chain, &spec, &base, st)).collect::<Result<_>>()?);
        stages.insert(format!("{st:?}"), json!(t));
    }
    eprintln!("stage times at 100% view, 2560x1440 + halo: {}", serde_json::to_string(&stages)?);

    let vary = |f: &dyn Fn(usize) -> gpu_pipeline::Params, from: Stage| -> Result<f64> {
        Ok(median((0..10).map(|i| r.chain_run(&chain, &spec, &f(i), from)).collect::<Result<_>>()?))
    };
    let s1 = vary(&|i| { let mut p = base; p.op[2][0] = 0.3 + 0.05 * (i % 5) as f32; p }, Stage::Combine)?;
    let s2 = vary(&|i| { let mut p = base; p.op[1][1] = 2.0 + (i % 2) as f32; p }, Stage::Blur)?;
    let s3 = vary(&|i| { let mut p = base; p.op[0][0] = 0.02 + 0.005 * (i % 5) as f32; p }, Stage::Denoise)?;
    let s4 = vary(&|i| { let mut p = base; p.wb[0] *= 1.0 + 0.01 * (i % 5) as f32; p }, Stage::Demosaic)?;
    let s5 = vary(&|i| { let mut p = base; p.exposure *= 1.0 + 0.02 * (i % 5) as f32; p }, Stage::Tone)?;
    // White balance applied after denoising: the demosaic keeps raw camera values, and the
    // multipliers fold into the camera matrix of the tone pass, so a WB change is downstream.
    let mut late = base;
    for row in 0..3 {
        for col in 0..3 {
            late.cam[row][col] = base.cam[row][col] * base.wb[col];
        }
    }
    late.wb = [1.0, 1.0, 1.0, 1.0];
    r.chain_run(&chain, &spec, &late, Stage::Demosaic)?;
    let s6 = vary(&|i| { let mut p = late; let k = 1.0 + 0.01 * (i % 5) as f32; for row in 0..3 { p.cam[row][0] *= k; } p }, Stage::Tone)?;
    r.chain_run(&chain, &spec, &base, Stage::Demosaic)?;
    eprintln!("setting changes at 100% view: exposure {s5:.1} ms | local contrast {s1:.1} | sharpen radius {s2:.1} | denoise strength {s3:.1} | white balance before denoise {s4:.1} | white balance after denoise {s6:.1}");

    // 3. How the cost of denoising grows with the search radius (patch radius 1).
    let mut sweep = serde_json::Map::new();
    for radius in [2.0f32, 3.0, 5.0, 7.0, 9.0] {
        let t = vary(&|i| { let mut p = base; p.op[0][2] = radius; p.op[0][0] = 0.02 + 0.005 * (i % 5) as f32; p }, Stage::Denoise)?;
        sweep.insert(format!("search_radius_{radius}"), json!(t));
    }
    eprintln!("denoise-strength change vs search radius: {}", serde_json::to_string(&sweep)?);

    drop(chain);

    // 4. Viewport size (each size is skipped if the GPU has no memory for it).
    let mut sizes = serde_json::Map::new();
    for (sw, sh) in [(1280u32, 720u32), (1920, 1080), (2560, 1440), (3840, 2160)] {
        if sw + 2 * HALO > w || sh + 2 * HALO > h {
            continue;
        }
        let spec = spec_for((w - sw) / 2, (h - sh) / 2, sw, sh);
        let Some(c) = r.try_chain(spec.rw, spec.rh, (sw * sh) as u64) else {
            sizes.insert(format!("{sw}x{sh}"), json!("does not fit (GPU memory or binding size limit)"));
            continue;
        };
        r.chain_run(&c, &spec, &base, Stage::Demosaic)?;
        let t3 = median((0..10).map(|i| { let mut p = base; p.op[0][0] = 0.02 + 0.005 * (i % 5) as f32; r.chain_run(&c, &spec, &p, Stage::Denoise) }).collect::<Result<_>>()?);
        let t4 = median((0..10).map(|i| { let mut p = base; p.wb[0] *= 1.0 + 0.01 * (i % 5) as f32; r.chain_run(&c, &spec, &p, Stage::Demosaic) }).collect::<Result<_>>()?);
        sizes.insert(format!("{sw}x{sh}"), json!({ "megapixels": (sw * sh) as f64 / 1e6, "denoise_change_ms": t3, "upstream_change_ms": t4 }));
    }
    eprintln!("viewport sizes: {}", serde_json::to_string(&sizes)?);

    // 5. Fit-to-screen: the chain on the downscaled image, with radii scaled to match.
    let mut fit_target = 2560;
    let (f, dw, dh, fc, prep) = loop {
        let (f, dw, dh, small, prep) = r.fit_prepare(fit_target)?;
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let c = r.chain_from(small, dw, dh, (dw * dh) as u64);
        if pollster::block_on(scope.pop()).is_none() {
            break (f, dw, dh, c, prep);
        }
        eprintln!("fit view at {fit_target} px wide does not fit in GPU memory, trying smaller");
        fit_target /= 2;
    };
    let fspec = Spec { x0: 0, y0: 0, rw: dw, rh: dh, halo_x: 0, halo_y: 0, inner_w: dw, inner_h: dh, out_x0: 0, out_y0: 0, out_stride: dw };
    let scaled = |p: gpu_pipeline::Params| {
        let mut p = p;
        p.op[0][2] = (p.op[0][2] / f as f32).ceil().max(1.0);
        p.op[1][1] = (p.op[1][1] / f as f32).ceil().max(1.0);
        p.op[2][1] = (p.op[2][1] / f as f32).ceil().max(1.0);
        p
    };
    let fbase = scaled(base);
    r.chain_run(&fc, &fspec, &fbase, Stage::Denoise)?;
    let fit_denoise = median((0..10).map(|i| { let mut p = fbase; p.op[0][0] = 0.02 + 0.005 * (i % 5) as f32; r.chain_run(&fc, &fspec, &p, Stage::Denoise) }).collect::<Result<_>>()?);
    let fit_local = median((0..10).map(|i| { let mut p = fbase; p.op[2][0] = 0.3 + 0.05 * (i % 5) as f32; r.chain_run(&fc, &fspec, &p, Stage::Combine) }).collect::<Result<_>>()?);
    eprintln!("fit view 1/{f} ({dw}x{dh}): cold prepare {prep:.0} ms; denoise change {fit_denoise:.1} ms; local contrast change {fit_local:.1} ms");

    drop(fc);

    // 6. Full-resolution export through the whole chain.
    let mut export_ms = Vec::new();
    let mut inner_rows = 0;
    for _ in 0..if (w * h) > 30_000_000 { 1 } else { 2 } {
        let (_, t, rows) = r.export_chain(&base, HALO, 150)?;
        export_ms.push(t);
        inner_rows = rows;
    }
    eprintln!("full export through the chain: {:.0} ms (bands of {inner_rows} rows + halo)", median(export_ms.clone()));

    let result = json!({
        "adapter": { "name": gpu.info.name, "backend": format!("{:?}", gpu.info.backend), "type": format!("{:?}", gpu.info.device_type) },
        "scene": scene.label, "megapixels": (w * h) as f64 / 1e6, "upload_ms": r.upload_ms,
        "operations": { "denoise": "non-local means, search radius 5 (11x11), patch 3x3", "sharpen": "unsharp mask, box radius 2", "local_contrast": "box blur radius 24", "mask": "radial exposure" },
        "accuracy_chain_vs_cpu": accuracy,
        "stage_ms_100_view": stages,
        "change_ms_100_view": { "exposure_tone_only": s5, "local_contrast": s1, "sharpen_radius": s2, "denoise_strength": s3, "white_balance_before_denoise": s4, "white_balance_after_denoise": s6 },
        "denoise_search_radius_sweep_ms": sweep,
        "viewport_sizes": sizes,
        "fit_view": { "factor": f, "size": [dw, dh], "cold_prepare_ms": prep, "denoise_change_ms": fit_denoise, "local_contrast_change_ms": fit_local },
        "export_chain_ms": median(export_ms),
    });
    if let Some(path) = get("--out") {
        std::fs::write(path, serde_json::to_string_pretty(&result)?)?;
    }
    Ok(())
}
