//! Runs the spike 1 measures on one adapter and prints them as JSON.
//!
//! usage: bench --adapter <substring of "backend name"> [--mp 24|61] [--out file.json] [--no-cpu]

use anyhow::Result;
use gpu_pipeline::{cpu, diff, gpu::{Gpu, Renderer}, scene};
use serde_json::json;
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn p95(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() as f64) * 0.95) as usize - 1]
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let adapter = get("--adapter").unwrap_or_else(|| "vulkan".into());
    let mp: u32 = get("--mp").and_then(|v| v.parse().ok()).unwrap_or(24);
    let run_cpu = !args.iter().any(|a| a == "--no-cpu");
    let (w, h) = match mp {
        61 => (9504u32, 6336u32),
        _ => (6000u32, 4000u32),
    };

    let gpu = Gpu::select(&adapter)?;
    eprintln!("adapter: {} [{:?}]", gpu.info.name, gpu.info.backend);

    // 1. Accuracy on a small image, against the CPU reference.
    let small = scene::synthetic(1536, 1024);
    let r_small = Renderer::new(&gpu, &small)?;
    let (gpu_small, _) = r_small.export(r_small.band_rows(1536, 1))?;
    let cpu_small = cpu::render_region(&small, 0, 0, 1536, 1024);
    let small_diff = diff(&gpu_small, &cpu_small);
    eprintln!("small image vs CPU reference: {small_diff:?}");

    // 2. The big scene.
    let t = Instant::now();
    let scene = scene::synthetic(w, h);
    eprintln!("scene {w}x{h} ({:.1} MP) built in {:.1}s", (w * h) as f64 / 1e6, t.elapsed().as_secs_f64());
    let r = Renderer::new(&gpu, &scene)?;
    let rows = r.band_rows(w, 1);

    let mut exports = Vec::new();
    let mut last = None;
    for _ in 0..3 {
        let (px, timing) = r.export(rows)?;
        exports.push(timing);
        last = Some(px);
    }
    let gpu_full = last.unwrap();
    let export_ms: Vec<f64> = exports.iter().map(|t| t.compute_ms).collect();
    let readback_ms: Vec<f64> = exports.iter().map(|t| t.readback_ms).collect();
    eprintln!("export: compute {:.0} ms, readback {:.0} ms (bands of {rows} rows)", median(export_ms.clone()), median(readback_ms.clone()));

    // 3. 100% view: a 2560x1440 viewport in the middle of the image.
    let (vw, vh) = (2560u32, 1440u32);
    let (vx, vy) = ((w - vw) / 2, (h - vh) / 2);
    let inter = r.storage(vw as u64 * vh as u64 * 16);
    let out = r.storage(vw as u64 * vh as u64 * 4);
    for _ in 0..3 {
        r.crop_recompute(vx, vy, vw, vh, &inter, &out)?;
    }
    let recompute: Vec<f64> = (0..30).map(|_| r.crop_recompute(vx, vy, vw, vh, &inter, &out)).collect::<Result<_>>()?;
    let tone_only: Vec<f64> = (0..100).map(|i| r.crop_tone_only(vw, vh, 1.0 + (i % 10) as f32 * 0.05, &inter, &out)).collect::<Result<_>>()?;
    // Accuracy of the crop path against the CPU reference on a 512x512 patch.
    r.crop_recompute(vx, vy, vw, vh, &inter, &out)?;
    let crop_px = r.read(&out, vw as u64 * vh as u64)?;
    let patch = 512u32;
    let mut gpu_patch = Vec::with_capacity((patch * patch) as usize);
    for row in 0..patch {
        let s = (row * vw) as usize;
        gpu_patch.extend_from_slice(&crop_px[s..s + patch as usize]);
    }
    let cpu_patch = cpu::render_region(&scene, vx, vy, patch, patch);
    let crop_diff = diff(&gpu_patch, &cpu_patch);
    eprintln!("100% view: recompute {:.1} ms (p95 {:.1}), tone only {:.2} ms; patch vs CPU {crop_diff:?}", median(recompute.clone()), p95(recompute.clone()), median(tone_only.clone()));

    // 4. Fit-to-screen.
    let mut fit = Vec::new();
    let (mut small_buf, mut fit_out) = (None, None);
    let (mut f, mut dw, mut dh) = (0, 0, 0);
    for _ in 0..3 {
        let (ff, ww, hh, t) = r.fit_cold(2560, &mut small_buf, &mut fit_out)?;
        (f, dw, dh) = (ff, ww, hh);
        fit.push(t);
    }
    let fit_tone: Vec<f64> = (0..100)
        .map(|i| r.fit_tone_only(dw, dh, 1.0 + (i % 10) as f32 * 0.05, small_buf.as_ref().unwrap(), fit_out.as_ref().unwrap()))
        .collect::<Result<_>>()?;
    eprintln!("fit (1/{f}, {dw}x{dh}): cold {:.0} ms, slider {:.2} ms", median(fit.clone()), median(fit_tone.clone()));

    // 5. The CPU reference: accuracy over the whole image, and speed.
    let cpu_result = if run_cpu {
        let t = Instant::now();
        let cpu_full = cpu::render_region(&scene, 0, 0, w, h);
        let cpu_ms = t.elapsed().as_secs_f64() * 1000.0;
        let d = diff(&gpu_full, &cpu_full);
        eprintln!("CPU reference (rayon, {} threads): {cpu_ms:.0} ms; full image vs GPU {d:?}", rayon::current_num_threads());
        Some(json!({ "threads": rayon::current_num_threads(), "full_ms": cpu_ms, "gpu_vs_cpu_full": d }))
    } else {
        None
    };

    let result = json!({
        "adapter": { "name": gpu.info.name, "backend": format!("{:?}", gpu.info.backend), "type": format!("{:?}", gpu.info.device_type), "driver": gpu.info.driver_info },
        "image": { "width": w, "height": h, "megapixels": (w * h) as f64 / 1e6 },
        "band_rows": rows,
        "accuracy_small_vs_cpu": small_diff,
        "export_compute_ms": median(export_ms),
        "export_readback_ms": median(readback_ms),
        "view_100": { "viewport": [vw, vh], "recompute_median_ms": median(recompute.clone()), "recompute_p95_ms": p95(recompute), "tone_only_median_ms": median(tone_only.clone()), "tone_only_p95_ms": p95(tone_only), "patch_vs_cpu": crop_diff },
        "view_fit": { "factor": f, "size": [dw, dh], "cold_ms": median(fit), "slider_median_ms": median(fit_tone.clone()), "slider_p95_ms": p95(fit_tone) },
        "cpu_reference": cpu_result,
    });
    let text = serde_json::to_string_pretty(&result)?;
    println!("{text}");
    if let Some(path) = get("--out") {
        std::fs::write(path, text)?;
    }
    Ok(())
}
