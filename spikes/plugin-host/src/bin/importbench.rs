//! A RAW decoder as an import plugin (rawler compiled to WebAssembly) against the same decoder
//! running natively, on real files. usage: importbench [samples dir] [--out result.json]
use plugin_host::*;
use rawler::{RawImageData, decoders::RawDecodeParams, rawsource::RawSource};
use serde_json::json;
use std::time::Instant;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.iter().skip(1).find(|a| !a.starts_with("--") && !a.ends_with(".json")).cloned().unwrap_or_else(|| "samples".into());
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).cloned();
    let eng = engine(false, false)?;
    let path = plugin_path("rawimport.wasm");
    let t = Instant::now();
    let module = load(&eng, &path)?;
    let compile_ms = ms(t);
    let bytes = module.serialize()?;
    let t = Instant::now();
    let _m = unsafe { wasmtime::Module::deserialize(&eng, &bytes)? };
    let load_ms = ms(t);
    println!("the import plugin: {:.1} MB of WebAssembly; compiling it takes {compile_ms:.0} ms; loading the compiled copy ({:.1} MB) takes {load_ms:.1} ms\n", std::fs::metadata(&path)?.len() as f64 / 1048576.0, bytes.len() as f64 / 1048576.0);
    let mut report = serde_json::Map::new();
    report.insert("compile_ms".into(), json!(compile_ms));
    report.insert("load_compiled_ms".into(), json!(load_ms));

    let mut files: Vec<_> = std::fs::read_dir(&dir)?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| ["CR3", "NEF", "ARW", "RAF", "RW2", "ORF"].contains(&x.to_string_lossy().to_uppercase().as_str()))).collect();
    files.sort();
    println!("{:<34} {:>8} {:>10} | {:>26} | {:>10} {:>8} {}", "file", "size", "native", "wasm: in + decode + out", "total", "ratio", "same pixels?");
    let mut rows = Vec::new();
    for f in &files {
        let data = std::fs::read(f)?;
        let native_t: Vec<f64> = (0..3).map(|_| { let src = RawSource::new_from_slice(&data); let t = Instant::now(); let img = rawler::decode(&src, &RawDecodeParams::default()).unwrap(); let d = ms(t); std::hint::black_box(img); d }).collect();
        let native = pct(&native_t, 0.5);
        let src = RawSource::new_from_slice(&data);
        let nimg = rawler::decode(&src, &RawDecodeParams::default()).unwrap();
        let RawImageData::Integer(nat) = &nimg.data else { continue };

        let t = Instant::now();
        let mut p = instantiate(&eng, &module, &Grants { memory_limit: Some(3 << 30), ..Default::default() })?;
        let inst_ms = ms(t);
        let t = Instant::now();
        let at = p.alloc(data.len())?;
        p.write(at, &data)?;
        let out_at = p.alloc(28)?;
        let t_in = ms(t);
        let import = p.func::<(u32, u32, u32), i32>("import")?;
        let t = Instant::now();
        let rc = import.call(&mut p.store, (at, data.len() as u32, out_at))?;
        let t_dec = ms(t);
        let mut hdr = [0u8; 28];
        p.read(out_at, &mut hdr)?;
        let v: Vec<u32> = hdr.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        let t = Instant::now();
        let mut back = vec![0u16; v[4] as usize];
        p.read(v[3], unsafe { std::slice::from_raw_parts_mut(back.as_mut_ptr() as *mut u8, back.len() * 2) })?;
        let t_out = ms(t);
        let same = rc == 0 && back.len() == nat.len() && back == *nat;
        let total = t_in + t_dec + t_out;
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        println!("{:<34} {:>5.0} MB {:>7.0} ms | {:>5.0} + {:>6.0} + {:>5.0} ms | {:>7.0} ms {:>7.2}x {}", name, data.len() as f64 / 1048576.0, native, t_in, t_dec, t_out, total, total / native, if same { "identical" } else { "DIFFERENT" });
        rows.push(json!({ "file": name, "mb": data.len() as f64 / 1048576.0, "native_ms": native, "wasm_in_ms": t_in, "wasm_decode_ms": t_dec, "wasm_out_ms": t_out, "wasm_total_ms": total, "ratio_decode": t_dec / native, "ratio_total": total / native, "instantiate_ms": inst_ms, "identical": same, "guest_memory_mb": p.memory.data_size(&p.store) as f64 / 1048576.0 }));
    }
    let ratios: Vec<f64> = rows.iter().map(|r| r["ratio_decode"].as_f64().unwrap()).collect();
    println!("\ndecoding alone: {:.2}x to {:.2}x the native time", ratios.iter().cloned().fold(f64::MAX, f64::min), ratios.iter().cloned().fold(0.0, f64::max));
    report.insert("files".into(), json!(rows));
    if let Some(o) = out { std::fs::write(o, serde_json::to_string_pretty(&report)?)?; }
    Ok(())
}
