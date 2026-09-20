//! Runs the spike 1 measures on real RAW files, on one adapter.
//!
//! usage: rawbench --adapter <words> [--samples dir] [--out file.json] [--no-cpu]

use anyhow::Result;
use gpu_pipeline::{cpu, diff, gpu::{Gpu, Renderer}, raw};
use serde_json::json;
use std::path::PathBuf;
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let adapter = get("--adapter").unwrap_or_else(|| "vulkan".into());
    let dir = PathBuf::from(get("--samples").unwrap_or_else(|| "samples".into()));
    let run_cpu = !args.iter().any(|a| a == "--no-cpu");
    let generic = args.iter().any(|a| a == "--generic");
    let previews = dir.join("previews");
    std::fs::create_dir_all(&previews)?;

    let gpu = Gpu::select(&adapter)?;
    eprintln!("adapter: {} [{:?}]", gpu.info.name, gpu.info.backend);

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let mut results = Vec::new();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let t = Instant::now();
        let scene = match raw::load(&path, generic) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{name}: SKIPPED ({e})");
                results.push(json!({ "file": name, "status": format!("unsupported: {e}") }));
                continue;
            }
        };
        let decode_ms = t.elapsed().as_secs_f64() * 1000.0;
        let (w, h) = (scene.width, scene.height);
        eprintln!("\n{name}: {}\n  decode {decode_ms:.0} ms, exposure x{:.2}", scene.label, scene.params.exposure);

        let r = Renderer::new(&gpu, &scene)?;
        let rows = r.band_rows(w, 1);
        let mut exports = Vec::new();
        let mut last = None;
        for _ in 0..3 {
            let (px, timing) = r.export(rows)?;
            exports.push(timing.compute_ms);
            last = Some(px);
        }
        let gpu_full = last.unwrap();
        let export_ms = median(exports);

        let (vw, vh) = (2560u32.min(w), 1440u32.min(h));
        let (vx, vy) = ((w - vw) / 2, (h - vh) / 2);
        let inter = r.storage(vw as u64 * vh as u64 * 16);
        let out = r.storage(vw as u64 * vh as u64 * 4);
        r.crop_recompute(vx, vy, vw, vh, &inter, &out)?;
        let recompute = median((0..20).map(|_| r.crop_recompute(vx, vy, vw, vh, &inter, &out)).collect::<Result<_>>()?);
        let tone = median((0..50).map(|i| r.crop_tone_only(vw, vh, scene.params.exposure * (1.0 + (i % 10) as f32 * 0.02), &inter, &out)).collect::<Result<_>>()?);

        let (mut small, mut fit_out) = (None, None);
        let (_, dw, dh, fit_cold) = r.fit_cold(2560, &mut small, &mut fit_out)?;
        let preview = r.read(fit_out.as_ref().unwrap(), dw as u64 * dh as u64)?;
        let bytes: Vec<u8> = preview.iter().flat_map(|p| p.to_le_bytes()).collect();
        let stem = format!("{}{}", path.file_stem().unwrap().to_string_lossy(), if generic { "-generic" } else { "" });
        image::save_buffer(previews.join(format!("{stem}.png")), &bytes, dw, dh, image::ExtendedColorType::Rgba8)?;

        let cpu_part = if run_cpu {
            let t = Instant::now();
            let cpu_full = cpu::render_region(&scene, 0, 0, w, h);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let d = diff(&gpu_full, &cpu_full);
            eprintln!("  CPU reference {ms:.0} ms; GPU vs CPU: {d:?}");
            Some(json!({ "ms": ms, "diff": d }))
        } else {
            None
        };
        eprintln!("  export {export_ms:.0} ms; 100% view recompute {recompute:.1} ms, tone only {tone:.2} ms; fit cold {fit_cold:.0} ms");
        results.push(json!({
            "file": name, "status": "ok", "label": scene.label, "megapixels": (w * h) as f64 / 1e6,
            "decode_ms": decode_ms, "export_ms": export_ms, "view_100_recompute_ms": recompute,
            "view_100_tone_only_ms": tone, "fit_cold_ms": fit_cold, "cpu": cpu_part,
        }));
    }

    let result = json!({
        "adapter": { "name": gpu.info.name, "backend": format!("{:?}", gpu.info.backend), "type": format!("{:?}", gpu.info.device_type) },
        "files": results,
    });
    let text = serde_json::to_string_pretty(&result)?;
    if let Some(path) = get("--out") {
        std::fs::write(path, &text)?;
    }
    Ok(())
}
