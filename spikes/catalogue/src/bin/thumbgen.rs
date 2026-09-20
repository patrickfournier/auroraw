//! Thumbnails: how fast they can be made, and whether to keep them as files or as database blobs.
//! usage: thumbgen [--count N] [--previews dir] [--out result.json]
use anyhow::Result;
use catalogue::{data_dir, evict, ms, pct};
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer, images::Image as FImage, PixelType};
use image::{ImageFormat, RgbImage};
use jpeg_encoder::{ColorType, Encoder};
use rayon::prelude::*;
use rusqlite::{Connection, params};
use serde_json::json;
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const LONG_EDGE: u32 = 256;

fn encode_jpeg(rgb: &[u8], w: u32, h: u32, q: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 * 1024);
    Encoder::new(&mut out, q).encode(rgb, w as u16, h as u16, ColorType::Rgb).unwrap();
    out
}

/// The same job without the four copies of the full-size image: the crop is done by the resizer.
fn make_thumb_lean(base: &[u8], variant: u32) -> (Vec<u8>, [f64; 3]) {
    let t = Instant::now();
    let img = image::load_from_memory_with_format(base, ImageFormat::Jpeg).unwrap().into_rgb8();
    let t_dec = ms(t.elapsed());
    let t = Instant::now();
    let mut r = fastrand::Rng::with_seed(variant as u64);
    let scale = 0.6 + 0.4 * r.f32();
    let (cw, ch) = ((img.width() as f32 * scale) as u32, (img.height() as f32 * scale) as u32);
    let (cx, cy) = (r.u32(0..=img.width() - cw), r.u32(0..=img.height() - ch));
    let (w, h) = if cw >= ch { (LONG_EDGE, (LONG_EDGE * ch / cw).max(1)) } else { ((LONG_EDGE * cw / ch).max(1), LONG_EDGE) };
    let (iw, ih) = (img.width(), img.height());
    let src = FImage::from_vec_u8(iw, ih, img.into_raw(), PixelType::U8x3).unwrap();
    let mut dst = FImage::new(w, h, PixelType::U8x3);
    let opts = ResizeOptions::new().crop(cx as f64, cy as f64, cw as f64, ch as f64).resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
    thread_local! { static RESIZER: std::cell::RefCell<Resizer> = std::cell::RefCell::new(Resizer::new()); }
    RESIZER.with(|rz| rz.borrow_mut().resize(&src, &mut dst, &opts).unwrap());
    let t_rs = ms(t.elapsed());
    let t = Instant::now();
    let jpg = encode_jpeg(dst.buffer(), w, h, 80);
    (jpg, [t_dec, t_rs, ms(t.elapsed())])
}

fn resize_rgb(src: &RgbImage, w: u32, h: u32) -> Vec<u8> {
    let s = FImage::from_vec_u8(src.width(), src.height(), src.as_raw().clone(), PixelType::U8x3).unwrap();
    let mut d = FImage::new(w, h, PixelType::U8x3);
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
    Resizer::new().resize(&s, &mut d, &opts).unwrap();
    d.into_vec()
}

/// The whole job for one thumbnail, from an embedded-preview-sized JPEG. Returns the bytes and
/// the times of (decode, crop and resize, encode).
fn make_thumb(base: &[u8], variant: u32) -> (Vec<u8>, [f64; 3]) {
    let t = Instant::now();
    let img = image::load_from_memory_with_format(base, ImageFormat::Jpeg).unwrap().to_rgb8();
    let t_dec = ms(t.elapsed());
    let t = Instant::now();
    // A crop of 60 to 100% at a position that depends on the variant, so no two are alike.
    let mut r = fastrand::Rng::with_seed(variant as u64);
    let scale = 0.6 + 0.4 * r.f32();
    let (cw, ch) = ((img.width() as f32 * scale) as u32, (img.height() as f32 * scale) as u32);
    let (cx, cy) = (r.u32(0..=img.width() - cw), r.u32(0..=img.height() - ch));
    let crop = image::imageops::crop_imm(&img, cx, cy, cw, ch).to_image();
    let (w, h) = if cw >= ch { (LONG_EDGE, (LONG_EDGE * ch / cw).max(1)) } else { ((LONG_EDGE * cw / ch).max(1), LONG_EDGE) };
    let small = resize_rgb(&crop, w, h);
    let t_rs = ms(t.elapsed());
    let t = Instant::now();
    let jpg = encode_jpeg(&small, w, h, 80);
    (jpg, [t_dec, t_rs, ms(t.elapsed())])
}

fn path_of_early(id: usize) -> PathBuf {
    data_dir().join("thumbs-files").join(format!("{:02x}", id % 256)).join(format!("{:02x}", (id / 256) % 256)).join(format!("{id}.jpg"))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let count: usize = get("--count").and_then(|v| v.parse().ok()).unwrap_or(100_000);
    let reads_only = args.iter().any(|a| a == "--reads-only");
    let previews = PathBuf::from(get("--previews").unwrap_or_else(|| "samples/previews".into()));
    let dir = data_dir();
    let mut out = serde_json::Map::new();

    // The sources: real developed images, re-encoded as JPEG previews of 1620 pixels wide, the size
    // of a typical embedded preview.
    let mut bases: Vec<Vec<u8>> = Vec::new();
    for e in std::fs::read_dir(&previews)? {
        let p = e?.path();
        if p.extension().is_some_and(|x| x == "png") && !p.to_string_lossy().contains("generic") {
            let img = image::open(&p)?.to_rgb8();
            let (w, h) = (1620u32, (1620.0 * img.height() as f64 / img.width() as f64) as u32);
            bases.push(encode_jpeg(&resize_rgb(&img, w, h), w, h, 85));
        }
    }
    println!("{} source previews, {:.0} KB each on average", bases.len(), bases.iter().map(|b| b.len()).sum::<usize>() as f64 / bases.len() as f64 / 1024.0);

    // 1. One thread: where the time goes.
    let samples: Vec<([f64; 3], usize)> = (0..60).map(|i| { let (b, t) = make_thumb(&bases[i % bases.len()], i as u32); (t, b.len()) }).collect();
    let avg = |k: usize| samples.iter().skip(5).map(|s| s.0[k]).sum::<f64>() / (samples.len() - 5) as f64;
    println!("one thumbnail on one core: decode {:.2} ms + crop and resize {:.2} ms + encode {:.2} ms = {:.2} ms", avg(0), avg(1), avg(2), avg(0) + avg(1) + avg(2));
    out.insert("one_core_ms".into(), json!({ "decode": avg(0), "resize": avg(1), "encode": avg(2), "total": avg(0) + avg(1) + avg(2) }));

    // 2. All of them, on every core.
    let t = Instant::now();
    let thumbs: Vec<Vec<u8>> = if reads_only { (0..300).map(|i| std::fs::read(path_of_early(i + 1)).unwrap()).collect() } else { (0..count).into_par_iter().map(|i| if args.iter().any(|a| a == "--lean") { make_thumb_lean(&bases[i % bases.len()], i as u32).0 } else { make_thumb(&bases[i % bases.len()], i as u32).0 }).collect() };
    let gen_s = t.elapsed().as_secs_f64();
    let bytes: usize = thumbs.iter().map(|t| t.len()).sum();
    let threads = rayon::current_num_threads();
    println!("{count} thumbnails in {gen_s:.1} s on {threads} threads: {:.0} per second, {:.0} per second per core; {:.1} KB each on average, {:.0} MB in total", count as f64 / gen_s, count as f64 / gen_s / threads as f64, bytes as f64 / count as f64 / 1024.0, bytes as f64 / 1048576.0);
    out.insert("generation".into(), json!({ "count": count, "seconds": gen_s, "threads": threads, "per_second": count as f64 / gen_s, "avg_kb": bytes as f64 / count as f64 / 1024.0, "total_mb": bytes as f64 / 1048576.0 }));

    if args.iter().any(|a| a == "--gen-only") {
        return Ok(());
    }

    // Decoding a thumbnail to pixels, for display.
    let dec: Vec<f64> = (0..300).map(|i| { let t = Instant::now(); let _ = image::load_from_memory_with_format(&thumbs[i], ImageFormat::Jpeg).unwrap().to_rgba8(); ms(t.elapsed()) }).collect();
    println!("decode a thumbnail for display: median {:.3} ms", pct(&dec, 0.5));
    out.insert("display_decode_ms".into(), json!(pct(&dec, 0.5)));

    // 3. Files: two levels of directories, 256 x 256.
    let files_root = dir.join("thumbs-files");
    let path_of = |id: usize| files_root.join(format!("{:02x}", id % 256)).join(format!("{:02x}", (id / 256) % 256)).join(format!("{id}.jpg"));
    let mut db_paths = Vec::new();
    for page in [4096, 32768] {
        db_paths.push((page, dir.join(format!("thumbs-{}k.db", page / 1024))));
    }
    if !reads_only {
        let _ = std::fs::remove_dir_all(&files_root);
        let t = Instant::now();
        (0..count).into_par_iter().for_each(|i| {
            let p = path_of(i + 1);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, &thumbs[i]).unwrap();
        });
        let files_write = t.elapsed().as_secs_f64();
        let (mut apparent, mut on_disk) = (0u64, 0u64);
        for i in 1..=count {
            let m = std::fs::metadata(path_of(i))?;
            apparent += m.len();
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                on_disk += m.blocks() * 512;
            }
            #[cfg(not(unix))]
            {
                on_disk += m.len().div_ceil(4096) * 4096;
            }
        }
        println!("\nfiles: written in {files_write:.1} s (16 threads); {:.0} MB of data, {:.0} MB on disk (blocks), {count} files", apparent as f64 / 1048576.0, on_disk as f64 / 1048576.0);
        out.insert("files".into(), json!({ "write_seconds": files_write, "data_mb": apparent as f64 / 1048576.0, "disk_mb": on_disk as f64 / 1048576.0, "count": count }));
        for (page, p) in &db_paths {
            for ext in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{ext}", p.display()));
            }
            let mut db = Connection::open(p)?;
            db.execute_batch(&format!("PRAGMA page_size={page}; PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; CREATE TABLE t(id INTEGER PRIMARY KEY, data BLOB NOT NULL);"))?;
            let t = Instant::now();
            let tx = db.transaction()?;
            {
                let mut st = tx.prepare("INSERT INTO t VALUES (?,?)")?;
                for (i, th) in thumbs.iter().enumerate() {
                    st.execute(params![i as i64 + 1, th])?;
                }
            }
            tx.commit()?;
            let secs = t.elapsed().as_secs_f64();
            drop(db);
            let size = std::fs::metadata(p)?.len();
            println!("blobs, page size {}: written in {secs:.1} s (one thread); {:.0} MB", page, size as f64 / 1048576.0);
            out.insert(format!("blobs_{}k", page / 1024), json!({ "write_seconds": secs, "size_mb": size as f64 / 1048576.0 }));
        }
    }

    // 5. Reading a page of 200 thumbnails: the next screen of the grid. Three patterns: the next
    // 200 by date (contiguous ids), a filtered view where every fifth photo matches (ascending,
    // spaced), and a random set. "Warm" reads the same set a second time.
    let mut rng = fastrand::Rng::with_seed(11);
    let contiguous: Vec<Vec<usize>> = (0..20).map(|_| { let s = 1 + rng.usize(0..count - 1200); (s..s + 200).collect() }).collect();
    let spaced: Vec<Vec<usize>> = (0..20).map(|_| { let s = 1 + rng.usize(0..count - 1200); (0..200).map(|k| s + k * 5 + rng.usize(0..3)).collect() }).collect();
    let scattered: Vec<Vec<usize>> = (0..20).map(|_| { let mut v: Vec<usize> = (0..200).map(|_| 1 + rng.usize(0..count)).collect(); v.sort(); v }).collect();
    println!("\nreading 200 thumbnails (median of 20):");
    println!("{:<12} {:<26} {:>12} {:>12}", "store", "ids", "cold cache", "warm cache");
    let read_files = |ids: &Vec<usize>| { let mut buf = Vec::new(); for &i in ids { buf.clear(); std::fs::File::open(path_of(i)).unwrap().read_to_end(&mut buf).unwrap(); } };
    let read_blobs = |ids: &Vec<usize>, db: &Connection| { let mut st = db.prepare_cached("SELECT data FROM t WHERE id=?").unwrap(); for &i in ids { let _b: Vec<u8> = st.query_row(params![i as i64], |r| r.get(0)).unwrap(); } };
    for (label, sets) in [("contiguous", &contiguous), ("every fifth (a filter)", &spaced), ("random, ascending", &scattered)] {
        let cold: Vec<f64> = sets.iter().map(|ids| { for &i in ids { evict(&path_of(i)); } let t = Instant::now(); read_files(ids); ms(t.elapsed()) }).collect();
        let warm: Vec<f64> = sets.iter().map(|ids| { read_files(ids); let t = Instant::now(); read_files(ids); ms(t.elapsed()) }).collect();
        println!("{:<12} {:<26} {:>9.1} ms {:>9.1} ms", "files", label, pct(&cold, 0.5), pct(&warm, 0.5));
        out.insert(format!("read_files_{}", label.split(' ').next().unwrap()), json!({ "cold_ms": pct(&cold, 0.5), "warm_ms": pct(&warm, 0.5) }));
        for (page, p) in &db_paths {
            let cold: Vec<f64> = sets.iter().map(|ids| { evict(p); let db = Connection::open(p).unwrap(); let t = Instant::now(); read_blobs(ids, &db); ms(t.elapsed()) }).collect();
            let db = Connection::open(p)?;
            let warm: Vec<f64> = sets.iter().map(|ids| { read_blobs(ids, &db); let t = Instant::now(); read_blobs(ids, &db); ms(t.elapsed()) }).collect();
            println!("{:<12} {:<26} {:>9.1} ms {:>9.1} ms", format!("blobs {}K", page / 1024), label, pct(&cold, 0.5), pct(&warm, 0.5));
            out.insert(format!("read_blobs_{}k_{}", page / 1024, label.split(' ').next().unwrap()), json!({ "cold_ms": pct(&cold, 0.5), "warm_ms": pct(&warm, 0.5) }));
        }
    }
    if let Some(o) = get("--out") {
        std::fs::write(o, serde_json::to_string_pretty(&out)?)?;
    }
    Ok(())
}
