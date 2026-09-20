//! A GPU operation plugin: its declaration, the checks the host makes on its shader, its place in
//! the pipeline, its run on the GPU, and its CPU twin in the sandbox. usage: gpubench [--out r.json]
use gpu_pipeline::{gpu::{Gpu, Renderer}, raw, scene};
use plugin_host::*;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Instant;

/// What the host will not even try to compile. Crude on purpose: it shows where a real check goes.
fn lint_wgsl(src: &str) -> std::result::Result<(), String> {
    if src.len() > 64 * 1024 { return Err("the shader is over 64 KB".into()); }
    let words: Vec<&str> = src.split(|c: char| !c.is_alphanumeric() && c != '_').collect();
    for banned in ["loop", "while"] {
        if words.contains(&banned) { return Err(format!("`{banned}` is not allowed: a shader that never ends cannot be stopped on a GPU")); }
    }
    if let Some(i) = src.find("@workgroup_size(") {
        let args: Vec<u32> = src[i + 16..].split(')').next().unwrap_or("").split(',').filter_map(|v| v.trim().parse().ok()).collect();
        if args.iter().product::<u32>() > 256 { return Err(format!("workgroup of {} invocations, the limit is 256", args.iter().product::<u32>())); }
    }
    if src.matches("@binding(").count() > 4 { return Err("more than four bindings".into()); }
    Ok(())
}

/// The default order of the pipeline's stages, and where a plugin may go in it.
const DEFAULT: [&str; 6] = ["demosaic", "denoise", "sharpen", "local-contrast", "tone", "output"];

fn place(decl: &Value) -> std::result::Result<Vec<String>, String> {
    let mut order: Vec<String> = DEFAULT.iter().map(|s| s.to_string()).collect();
    let idx = |name: &str, order: &Vec<String>| order.iter().position(|s| s == name).ok_or_else(|| format!("unknown stage `{name}`"));
    let after = decl["after"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>()).unwrap_or_default();
    let before = decl["before"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>()).unwrap_or_default();
    let lo = after.iter().map(|n| idx(n, &order)).collect::<std::result::Result<Vec<_>, _>>()?.into_iter().max().map(|i| i + 1).unwrap_or(0);
    let hi = before.iter().map(|n| idx(n, &order)).collect::<std::result::Result<Vec<_>, _>>()?.into_iter().min().unwrap_or(order.len());
    if lo > hi { return Err(format!("no room: it must come after stage {} and before stage {}", lo - 1, hi)); }
    // As late as its constraints allow: expensive, rarely-changed operations belong early, but a
    // plugin with no further hint goes at the end of its window.
    order.insert(hi, decl["id"].as_str().unwrap_or("plugin").to_string());
    Ok(order)
}

/// The pipeline crate reports errors with anyhow; the host uses wasmtime's.
fn ah<T>(r: anyhow::Result<T>) -> Result<T> {
    r.map_err(|e| wasmtime::format_err!("{e:#}"))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).cloned();
    let mut report = serde_json::Map::new();

    // 1. Load the plugin and read its declaration.
    let eng = engine(false, false)?;
    let module = load(&eng, &plugin_path("gpuop.wasm"))?;
    let mut p = instantiate(&eng, &module, &Grants::default())?;
    let packed = p.func::<(), u64>("describe")?.call(&mut p.store, ())?;
    let (at, len) = ((packed >> 32) as u32, (packed & 0xffff_ffff) as usize);
    let mut bytes = vec![0u8; len];
    p.read(at, &mut bytes)?;
    let decl: Value = serde_json::from_slice(&bytes)?;
    println!("plugin {} v{}: family {}, panel {}, stage {}, permissions {:?}, parameters {}", decl["id"], decl["version"], decl["family"], decl["panel"], decl["stage"], decl["permissions"], decl["params"][0]["name"]);
    let wgsl = decl["gpu"]["wgsl"].as_str().unwrap().to_string();

    // 2. Checks and placement.
    println!("\nchecks on the shader: {:?}", lint_wgsl(&wgsl));
    let order = place(&decl).map_err(|e| wasmtime::format_err!("{e}"))?;
    println!("placement from the declaration (after demosaic, before tone): {}", order.join(" -> "));
    let mut placement_cases = Vec::new();
    for (name, after, before) in [("after the output, before demosaic", json!(["output"]), json!(["demosaic"])), ("a stage that does not exist", json!(["hologram"]), json!([])), ("no constraints at all", json!([]), json!([]))] {
        let d = json!({ "id": "test", "after": after, "before": before });
        let r = place(&d);
        println!("  {name}: {}", match &r { Ok(o) => o.join(" -> "), Err(e) => format!("refused: {e}") });
        placement_cases.push(json!({ "case": name, "result": r.map(|o| o.join(" -> ")).unwrap_or_else(|e| format!("refused: {e}")) }));
    }
    report.insert("placement_cases".into(), json!(placement_cases));

    // 3. The shader on the real GPU.
    let gpu = ah(Gpu::best())?;
    let sc = if Path::new("samples/Nikon-D850-14bit-compressed.NEF").exists() { ah(raw::load(Path::new("samples/Nikon-D850-14bit-compressed.NEF"), false))? } else { scene::synthetic(6000, 4000) };
    let r = ah(Renderer::new(&gpu, &sc))?;
    println!("\nadapter: {} [{:?}]", gpu.info.name, gpu.info.backend);
    let t = Instant::now();
    let op = r.compile_plugin_op(&wgsl).map_err(|e| wasmtime::format_err!("{e}"))?;
    let compile_ms = ms(t);
    let (w, h) = (1920u32, 1080u32);
    let (x0, y0) = ((sc.width - w) / 2, (sc.height - h) / 2);
    let a = r.storage(w as u64 * h as u64 * 16);
    let b = r.storage(w as u64 * h as u64 * 16);
    ah(r.demosaic_region(&a, x0, y0, w, h))?;
    let mut params = [0f32; 16];
    params[0] = 1.5;
    let gpu_ms: Vec<f64> = (0..30).map(|_| r.run_plugin_op(&op, &a, &b, w, h, &params).unwrap()).collect();
    let gpu_out = ah(r.read_f32(&b, w as u64 * h as u64 * 4))?;
    // The same work by the CPU twin, in the sandbox, on the same input.
    let input = ah(r.read_f32(&a, w as u64 * h as u64 * 4))?;
    let px = p.alloc(input.len() * 4)?;
    p.write(px, f32_bytes(&input))?;
    let f = p.func::<(u32, u32, u32, f32), ()>("process")?;
    let t = Instant::now();
    f.call(&mut p.store, (px, w, h, 1.5))?;
    let cpu_ms = ms(t);
    let mut cpu_out = vec![0f32; input.len()];
    p.read(px, f32_bytes_mut(&mut cpu_out))?;
    let (mut worst, mut worst_rel) = (0f32, 0f32);
    for (g, c) in gpu_out.iter().zip(&cpu_out) {
        let d = (g - c).abs();
        worst = worst.max(d);
        worst_rel = worst_rel.max(d / c.abs().max(1e-3));
    }
    let changed = input.iter().zip(&gpu_out).filter(|(i, o)| (*i - *o).abs() > 1e-4).count() as f64 / input.len() as f64;
    println!("compiled and validated in {compile_ms:.1} ms; on the GPU: {:.2} ms for {w}x{h} (median of 30); CPU twin in the sandbox: {cpu_ms:.1} ms", pct(&gpu_ms, 0.5));
    println!("GPU against CPU twin: worst absolute difference {worst:.2e}, worst relative {worst_rel:.2e}; {:.0}% of the values were changed by the operation", changed * 100.0);
    report.insert("run".into(), json!({ "compile_ms": compile_ms, "gpu_ms": pct(&gpu_ms, 0.5), "cpu_twin_ms": cpu_ms, "worst_abs_diff": worst, "worst_rel_diff": worst_rel, "fraction_changed": changed, "adapter": gpu.info.name }));

    // 4. Shaders that must be refused.
    println!("\nshaders that must be refused:");
    let good = wgsl.clone();
    let cases: Vec<(&str, String)> = vec![
        ("a syntax error", good.replace("let i = gid.y", "let i == gid.y")),
        ("a store into a read-only buffer", good.replace("var<storage, read_write> dst", "var<storage, read> dst")),
        ("no entry point", good.replace("fn main(", "fn other(")),
        ("an unbounded loop", good.replace("let c = src[i];", "var k = 0u; loop { k = k + 1u; if (k > 4u) { break; } } let c = src[i];")),
        ("a workgroup of 1024 invocations", good.replace("@workgroup_size(16, 16)", "@workgroup_size(32, 32)")),
        ("an undeclared variable", good.replace("p.op0.x", "p.nonsense")),
    ];
    let mut refused = Vec::new();
    for (name, src) in &cases {
        let verdict = match lint_wgsl(src) {
            Err(e) => format!("refused by the host's check: {e}"),
            Ok(()) => match r.compile_plugin_op(src) {
                Err(e) => {
                    let cause: String = e.lines().map(|l| l.trim()).find(|l| !l.is_empty() && !l.starts_with("Validation Error") && !l.starts_with("Caused by") && !l.starts_with("In ") && !l.starts_with("label")).unwrap_or("").chars().take(84).collect();
                    format!("refused by the GPU driver's validation: {cause}")
                }
                Ok(_) => "ACCEPTED".into(),
            },
        };
        println!("  {name:<36} {verdict}");
        refused.push(json!({ "case": name, "verdict": verdict }));
    }
    report.insert("refused".into(), json!(refused));
    println!("\nthe host and the GPU are still fine: {}", r.run_plugin_op(&op, &a, &b, w, h, &params).is_ok());
    if let Some(o) = out { std::fs::write(o, serde_json::to_string_pretty(&report)?)?; }
    Ok(())
}
