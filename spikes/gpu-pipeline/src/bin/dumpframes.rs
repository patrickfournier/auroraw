//! Writes a pan across a RAW file as RGBA8 frames, so toolkits written in other languages can
//! display exactly the images the Rust viewers display.
//!
//! usage: dumpframes --view WxH [--count 60] [--file raw] --out frames.bin
//! Format: three little-endian u32 (width, height, count), then count * width * height * 4 bytes.

use anyhow::Result;
use gpu_pipeline::{gpu::{Gpu, Renderer}, raw, scene};
use std::io::Write;
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let (vw, vh): (u32, u32) = get("--view").and_then(|v| v.split_once('x').map(|(a, b)| (a.parse().unwrap(), b.parse().unwrap()))).unwrap_or((1340, 964));
    let count: u32 = get("--count").and_then(|v| v.parse().ok()).unwrap_or(60);
    let out = get("--out").unwrap_or_else(|| "frames.bin".into());
    let gpu = Gpu::best()?;
    let file = get("--file").unwrap_or_else(|| "samples/Nikon-D850-14bit-compressed.NEF".into());
    let sc = if Path::new(&file).exists() { raw::load(Path::new(&file), false)? } else { scene::synthetic(6000, 4000) };
    let r = Renderer::new(&gpu, &sc)?;
    let vp = r.view_path(vw, vh);
    let (mx, my) = (sc.width - vw, sc.height - vh);
    let mut f = std::io::BufWriter::new(std::fs::File::create(&out)?);
    for v in [vw, vh, count] {
        f.write_all(&v.to_le_bytes())?;
    }
    let mut buf = vec![0u8; (vw * vh * 4) as usize];
    for i in 0..count {
        let t = i as f64 / (count - 1).max(1) as f64;
        r.render_view(&vp, (mx as f64 * t) as u32, (my as f64 * t) as u32, &mut buf)?;
        f.write_all(&buf)?;
    }
    println!("{count} frames of {vw}x{vh} from {} written to {out}", sc.label);
    Ok(())
}
