//! A quick check that every shader compiles and agrees with the CPU reference on one adapter.
//! Runs in a few seconds, even on a software adapter, so continuous integration can run it on
//! every platform. Exits with a non-zero status if anything is wrong.
//!
//! usage: smoke --adapter <words>

use anyhow::Result;
use gpu_pipeline::{Params, cpu, diff, gpu::{Gpu, Renderer, Spec, Stage}, scene};

/// The Fujifilm X-T50's 6x6 pattern, as reported by the decoder.
const XTRANS: &str = "GGRGGBGGBGGRBRGRBGGGBGGRGGRGGBRBGBRG";

fn check(name: &str, d: gpu_pipeline::Diff, failed: &mut bool) {
    let ok = d.max_level <= 1 && d.fraction_over_one_level <= 0.001;
    println!("{:<44} max {} level(s), {:.4}% over one level  {}", name, d.max_level, d.fraction_over_one_level * 100.0, if ok { "ok" } else { "FAILED" });
    *failed |= !ok;
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let adapter = args.iter().position(|a| a == "--adapter").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| "vulkan".into());
    let gpu = Gpu::select(&adapter)?;
    println!("adapter: {} [{:?}, {:?}]", gpu.info.name, gpu.info.backend, gpu.info.device_type);
    let mut failed = false;

    // Bayer demosaicing (Malvar-He-Cutler), tone and display.
    let bayer = scene::synthetic(512, 384);
    let r = Renderer::new(&gpu, &bayer)?; // compiles every shader
    let (px, _) = r.export(r.band_rows(512, 1))?;
    check("Bayer: demosaic + tone", diff(&px, &cpu::render_region(&bayer, 0, 0, 512, 384)), &mut failed);

    // The fit-to-screen path (box downscale).
    let (mut small, mut out) = (None, None);
    r.fit_cold(128, &mut small, &mut out)?;
    println!("{:<44} ran  ok", "fit-to-screen (downscale pass)");

    // Generic demosaicing, with the X-Trans pattern and with Bayer tiled up.
    for (name, table) in [
        ("X-Trans: generic demosaic", XTRANS.chars().map(|c| match c { 'R' => 0u32, 'G' => 1, _ => 2 }).collect::<Vec<_>>()),
        ("Bayer tiled: generic demosaic", (0..36).map(|i| [[0u32, 1], [1, 2]][(i / 6) & 1][(i % 6) & 1]).collect()),
    ] {
        let mut generic = scene::synthetic(512, 384);
        generic.cfa6 = Some(table);
        let r = Renderer::new(&gpu, &generic)?;
        let (px, _) = r.export(r.band_rows(512, 1))?;
        check(name, diff(&px, &cpu::render_region(&generic, 0, 0, 512, 384)), &mut failed);
    }

    // The heavy chain: denoise, blurs, combine, tone, on a small haloed region.
    let (halo, rw, rh) = (8u32, 64u32, 64u32);
    let (x0, y0) = (200u32, 150u32);
    let base: Params = r.scene_params;
    let spec = Spec { x0: x0 - halo, y0: y0 - halo, rw: rw + 2 * halo, rh: rh + 2 * halo, halo_x: halo, halo_y: halo, inner_w: rw, inner_h: rh, out_x0: 0, out_y0: 0, out_stride: rw };
    let chain = r.chain(spec.rw, spec.rh, (rw * rh) as u64);
    r.chain_run(&chain, &spec, &base, Stage::Demosaic)?;
    let px = r.read(&chain.out, (rw * rh) as u64)?;
    check("Chain: denoise, blurs, combine, tone", diff(&px, &cpu::chain_region(&bayer, x0, y0, rw, rh, halo)), &mut failed);

    if failed {
        println!("SMOKE TEST FAILED");
        std::process::exit(1);
    }
    println!("smoke test passed");
    Ok(())
}
