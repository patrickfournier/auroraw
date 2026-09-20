//! Operation plugins in WebAssembly against the same kernels running natively.
//! usage: opbench [--out result.json]
use plugin_host::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn tile(w: usize, h: usize, seed: u64) -> Vec<f32> {
    let mut r = fastrand::Rng::with_seed(seed);
    (0..w * h * 4).map(|_| r.f32() * 1.5).collect()
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).cloned();
    let mut report = serde_json::Map::new();
    let (w, h) = (2048usize, 2048usize);
    let src = tile(w, h, 1);
    println!("tile {w}x{h} ({:.1} MP, {:.0} MB as f32 RGBA)\n", (w * h) as f64 / 1e6, (w * h * 16) as f64 / 1048576.0);

    for (label, file) in [("wasm scalar", "ops.wasm"), ("wasm SIMD128", "../../../target-simd/wasm32-wasip1/release/ops.wasm")] {
        if !plugin_path(file).exists() { println!("== {label}: not built, skipped\n"); continue; }
        let eng = engine(false, false)?;
        let path = plugin_path(file);
        let t = Instant::now();
        let module = load(&eng, &path)?;
        let compile_ms = ms(t);
        let bytes = module.serialize()?;
        let t = Instant::now();
        let _m2 = unsafe { wasmtime::Module::deserialize(&eng, &bytes)? };
        let deser_ms = ms(t);
        let inst: Vec<f64> = (0..20).map(|_| { let t = Instant::now(); let _p = instantiate(&eng, &module, &Grants::default()).unwrap(); ms(t) }).collect();
        println!("== {label}: {:.0} KB; compile {compile_ms:.1} ms; load a compiled copy {deser_ms:.2} ms ({:.0} KB); instantiate {:.3} ms", std::fs::metadata(&path)?.len() as f64 / 1024.0, bytes.len() as f64 / 1024.0, pct(&inst, 0.5));
        let mut r = serde_json::Map::new();
        r.insert("compile_ms".into(), json!(compile_ms));
        r.insert("load_compiled_ms".into(), json!(deser_ms));
        r.insert("instantiate_ms".into(), json!(pct(&inst, 0.5)));

        let mut p = instantiate(&eng, &module, &Grants::default())?;
        let noop = p.func::<(u32, u32), u32>("noop")?;
        let calls: Vec<f64> = (0..2000).map(|_| { let t = Instant::now(); noop.call(&mut p.store, (1, 1)).unwrap(); ms(t) * 1000.0 }).collect();
        println!("   a call that does nothing: median {:.2} us, p99 {:.2} us", pct(&calls, 0.5), pct(&calls, 0.99));
        r.insert("call_overhead_us".into(), json!(pct(&calls, 0.5)));

        let ptr = p.alloc(w * h * 16)?;
        let ptr2 = p.alloc(w * h * 16)?;
        // 1. tone_heavy, in place.
        let mut nat = src.clone();
        let nat_t: Vec<f64> = (0..5).map(|_| { nat.copy_from_slice(&src); let t = Instant::now(); kernels::tone_heavy(&mut nat, w, h, 0.3); ms(t) }).collect();
        let f = p.func::<(u32, u32, u32, f32), ()>("op_tone_heavy")?;
        let mut got = vec![0f32; w * h * 4];
        let (mut t_in, mut t_call, mut t_out) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..6 {
            let t = Instant::now(); p.write(ptr, f32_bytes(&src))?; let a = ms(t);
            let t = Instant::now(); f.call(&mut p.store, (ptr, w as u32, h as u32, 0.3))?; let b = ms(t);
            let t = Instant::now(); p.read(ptr, f32_bytes_mut(&mut got))?; let c = ms(t);
            if i > 0 { t_in.push(a); t_call.push(b); t_out.push(c); }
        }
        let native = pct(&nat_t, 0.5);
        println!("   tone (powf and sqrt per pixel): native {native:.1} ms | wasm: copy in {:.1} + run {:.1} + copy out {:.1} = {:.1} ms | run alone x{:.2}, with the copies x{:.2} | worst difference {:.1e}", pct(&t_in, 0.5), pct(&t_call, 0.5), pct(&t_out, 0.5), pct(&t_in, 0.5) + pct(&t_call, 0.5) + pct(&t_out, 0.5), pct(&t_call, 0.5) / native, (pct(&t_in, 0.5) + pct(&t_call, 0.5) + pct(&t_out, 0.5)) / native, max_diff(&got, &nat));
        r.insert("tone".into(), json!({ "native_ms": native, "in_ms": pct(&t_in, 0.5), "run_ms": pct(&t_call, 0.5), "out_ms": pct(&t_out, 0.5), "run_ratio": pct(&t_call, 0.5) / native, "total_ratio": (pct(&t_in, 0.5) + pct(&t_call, 0.5) + pct(&t_out, 0.5)) / native, "max_diff": max_diff(&got, &nat) }));

        // 2. conv5, from one buffer to another.
        let mut nat_dst = vec![0f32; w * h * 4];
        let nat_t: Vec<f64> = (0..5).map(|_| { let t = Instant::now(); kernels::conv5(&src, &mut nat_dst, w, h); ms(t) }).collect();
        let f = p.func::<(u32, u32, u32, u32), ()>("op_conv5")?;
        let (mut t_in, mut t_call, mut t_out) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..6 {
            let t = Instant::now(); p.write(ptr, f32_bytes(&src))?; let a = ms(t);
            let t = Instant::now(); f.call(&mut p.store, (ptr, ptr2, w as u32, h as u32))?; let b = ms(t);
            let t = Instant::now(); p.read(ptr2, f32_bytes_mut(&mut got))?; let c = ms(t);
            if i > 0 { t_in.push(a); t_call.push(b); t_out.push(c); }
        }
        let native = pct(&nat_t, 0.5);
        println!("   5x5 convolution: native {native:.1} ms | wasm: copy in {:.1} + run {:.1} + copy out {:.1} = {:.1} ms | run alone x{:.2}, with the copies x{:.2} | worst difference {:.1e}", pct(&t_in, 0.5), pct(&t_call, 0.5), pct(&t_out, 0.5), pct(&t_in, 0.5) + pct(&t_call, 0.5) + pct(&t_out, 0.5), pct(&t_call, 0.5) / native, (pct(&t_in, 0.5) + pct(&t_call, 0.5) + pct(&t_out, 0.5)) / native, max_diff(&got, &nat_dst));
        r.insert("conv5".into(), json!({ "native_ms": native, "in_ms": pct(&t_in, 0.5), "run_ms": pct(&t_call, 0.5), "out_ms": pct(&t_out, 0.5), "run_ratio": pct(&t_call, 0.5) / native, "total_ratio": (pct(&t_in, 0.5) + pct(&t_call, 0.5) + pct(&t_out, 0.5)) / native }));

        // 3. Almost no arithmetic: the cost of the boundary itself.
        let nat_t: Vec<f64> = (0..20).map(|_| { let t = Instant::now(); std::hint::black_box(kernels::checksum(&src)); ms(t) }).collect();
        let f = p.func::<(u32, u32, u32), f32>("op_checksum")?;
        let t_call: Vec<f64> = (0..20).map(|_| { let t = Instant::now(); f.call(&mut p.store, (ptr, w as u32, h as u32)).unwrap(); ms(t) }).collect();
        println!("   a sparse pass over the memory: native {:.2} ms, wasm {:.2} ms", pct(&nat_t, 0.5), pct(&t_call, 0.5));
        report.insert(label.into(), serde_json::Value::Object(r));
    }

    // Watching the plugin costs something: epoch interruption and fuel.
    println!("\n== the cost of the safety nets (wasm scalar, 5x5 convolution)");
    let mut safety = serde_json::Map::new();
    for (name, epoch, fuel) in [("no protection", false, false), ("epoch interruption", true, false), ("fuel metering", false, true)] {
        let eng = engine(epoch, fuel)?;
        let module = load(&eng, &plugin_path("ops.wasm"))?;
        let mut p = instantiate(&eng, &module, &Grants::default())?;
        if epoch { p.store.set_epoch_deadline(u64::MAX / 2); }
        if fuel { p.store.set_fuel(u64::MAX / 2)?; }
        let ptr = p.alloc(w * h * 16)?;
        let ptr2 = p.alloc(w * h * 16)?;
        p.write(ptr, f32_bytes(&src))?;
        let f = p.func::<(u32, u32, u32, u32), ()>("op_conv5")?;
        f.call(&mut p.store, (ptr, ptr2, w as u32, h as u32))?;
        let t: Vec<f64> = (0..6).map(|_| { let t = Instant::now(); f.call(&mut p.store, (ptr, ptr2, w as u32, h as u32)).unwrap(); ms(t) }).collect();
        println!("   {name:<20} {:.1} ms", pct(&t, 0.5));
        safety.insert(name.into(), json!(pct(&t, 0.5)));
    }
    report.insert("safety_nets_conv5_ms".into(), serde_json::Value::Object(safety));

    // Many tiles on many threads: 16 tiles of 1 MP, each thread with its own instance.
    println!("\n== 16 tiles of 1 MP (16 MB each) on N threads: wall time in ms, tone kernel");
    let (tw, th) = (1024usize, 1024usize);
    let tsrc = tile(tw, th, 2);
    let eng = engine(false, false)?;
    let module = load(&eng, &plugin_path("ops.wasm"))?;
    let mut scaling = Vec::new();
    println!("{:>8} {:>12} {:>12} {:>10}", "threads", "native", "wasm", "ratio");
    for threads in [1usize, 2, 4, 8, 16] {
        let next = AtomicUsize::new(0);
        let t = Instant::now();
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    let mut buf = tsrc.clone();
                    while next.fetch_add(1, Ordering::Relaxed) < 16 {
                        buf.copy_from_slice(&tsrc);
                        kernels::tone_heavy(&mut buf, tw, th, 0.3);
                    }
                });
            }
        });
        let native = ms(t);
        let next = AtomicUsize::new(0);
        let t = Instant::now();
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    let mut p = instantiate(&eng, &module, &Grants::default()).unwrap();
                    let ptr = p.alloc(tw * th * 16).unwrap();
                    let f = p.func::<(u32, u32, u32, f32), ()>("op_tone_heavy").unwrap();
                    let mut back = vec![0f32; tw * th * 4];
                    while next.fetch_add(1, Ordering::Relaxed) < 16 {
                        p.write(ptr, f32_bytes(&tsrc)).unwrap();
                        f.call(&mut p.store, (ptr, tw as u32, th as u32, 0.3)).unwrap();
                        p.read(ptr, f32_bytes_mut(&mut back)).unwrap();
                    }
                });
            }
        });
        let wasm = ms(t);
        println!("{threads:>8} {native:>10.0} ms {wasm:>10.0} ms {:>9.2}x", wasm / native);
        scaling.push(json!({ "threads": threads, "native_ms": native, "wasm_ms": wasm }));
    }
    report.insert("scaling_16_tiles".into(), json!(scaling));
    if let Some(o) = out { std::fs::write(o, serde_json::to_string_pretty(&report)?)?; }
    Ok(())
}
