//! Spike 2, part 1: what does it cost to get a developed viewport from the GPU to the CPU as
//! RGBA8, independent of any toolkit? Pans across a real RAW file, frame by frame.
//!
//! usage: presentation --adapter <words> [--file path]

use anyhow::Result;
use gpu_pipeline::{gpu::{Gpu, Renderer}, raw, scene};
use serde_json::json;
use std::path::Path;

fn pct(v: &[f64], q: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() as f64 * q) as usize).min(s.len() - 1)]
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let gpu = Gpu::select(&get("--adapter").unwrap_or_else(|| "vulkan".into()))?;
    let scene = match get("--file") {
        Some(f) if f != "synthetic" => raw::load(Path::new(&f), false)?,
        _ => scene::synthetic(6000, 4000),
    };
    let r = Renderer::new(&gpu, &scene)?;
    println!("adapter: {} [{:?}] | {} ({}x{})", gpu.info.name, gpu.info.backend, scene.label, scene.width, scene.height);
    let mut results = serde_json::Map::new();
    for (vw, vh) in [(1200u32, 800u32), (1872, 1064), (2560, 1440)] {
        let vp = r.view_path(vw, vh);
        let mut buf = vec![0u8; (vw * vh * 4) as usize];
        // Pan across the image along a diagonal, one step per frame.
        let frames = 200u32;
        let (max_x, max_y) = (scene.width - vw, scene.height - vh);
        let (mut gpu_ms, mut read_ms) = (Vec::new(), Vec::new());
        for i in 0..frames + 10 {
            let t = (i as f64 / frames as f64) * 2.0 % 2.0;
            let t = if t > 1.0 { 2.0 - t } else { t };
            let (g, rd) = r.render_view(&vp, (max_x as f64 * t) as u32, (max_y as f64 * t) as u32, &mut buf)?;
            if i >= 10 {
                gpu_ms.push(g);
                read_ms.push(rd);
            }
        }
        let total: Vec<f64> = gpu_ms.iter().zip(&read_ms).map(|(a, b)| a + b).collect();
        println!(
            "{vw}x{vh} ({:.1} MP, {:.1} MB): GPU {:.2} ms, readback {:.2} ms, total median {:.2} ms, p95 {:.2}, p99 {:.2}",
            (vw * vh) as f64 / 1e6, (vw * vh * 4) as f64 / 1e6, pct(&gpu_ms, 0.5), pct(&read_ms, 0.5), pct(&total, 0.5), pct(&total, 0.95), pct(&total, 0.99)
        );
        results.insert(format!("{vw}x{vh}"), json!({ "gpu_ms": pct(&gpu_ms, 0.5), "readback_ms": pct(&read_ms, 0.5), "total_median_ms": pct(&total, 0.5), "total_p95_ms": pct(&total, 0.95), "total_p99_ms": pct(&total, 0.99) }));
    }
    if let Some(path) = get("--out") {
        std::fs::write(path, serde_json::to_string_pretty(&json!({ "adapter": gpu.info.name, "backend": format!("{:?}", gpu.info.backend), "scene": scene.label, "views": results }))?)?;
    }
    Ok(())
}
