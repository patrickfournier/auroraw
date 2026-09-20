//! What does it cost to read a RAW file's metadata (no pixels)? The alternative to caching it in
//! the sidecar. usage: exifcost <dir of raw files>
use catalogue::{evict, ms, pct};
use rawler::decoders::RawDecodeParams;
use rawler::rawsource::RawSource;
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "samples".into());
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().is_some_and(|x| ["CR3", "NEF", "ARW", "RAF", "RW2", "ORF"].contains(&x.to_string_lossy().to_uppercase().as_str()))).collect();
    files.sort();
    println!("{:<34} {:>12} {:>12} {:>12}", "file", "cold", "warm", "bytes read?");
    let (mut all_cold, mut all_warm) = (Vec::new(), Vec::new());
    for f in &files {
        let mut read = |cold: bool| {
            if cold { evict(f); }
            let t = Instant::now();
            let src = RawSource::new(f).unwrap();
            let dec = rawler::get_decoder(&src).unwrap();
            let md = dec.raw_metadata(&src, &RawDecodeParams::default()).unwrap();
            let _ = (md.exif.iso_speed, md.exif.fnumber, md.model.clone());
            ms(t.elapsed())
        };
        let cold: Vec<f64> = (0..5).map(|_| read(true)).collect();
        let warm: Vec<f64> = (0..20).map(|_| read(false)).collect();
        println!("{:<34} {:>9.2} ms {:>9.2} ms", f.file_name().unwrap().to_string_lossy(), pct(&cold, 0.5), pct(&warm, 0.5));
        all_cold.push(pct(&cold, 0.5));
        all_warm.push(pct(&warm, 0.5));
    }
    let (c, w) = (all_cold.iter().sum::<f64>() / all_cold.len() as f64, all_warm.iter().sum::<f64>() / all_warm.len() as f64);
    println!("average: cold {c:.2} ms, warm {w:.2} ms per file, so 100,000 files: cold {:.0} s, warm {:.0} s on one thread; on 8 cores about {:.0} s cold, {:.0} s warm (cold is limited by the disk)", c * 100.0, w * 100.0, c * 100.0 / 8.0, w * 100.0 / 8.0);
}
