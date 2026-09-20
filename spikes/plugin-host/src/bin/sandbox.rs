//! What the host does when a plugin crashes, loops, hoards memory, or reaches for files, the
//! network, or other processes. usage: sandbox [--out result.json]
use plugin_host::*;
use serde_json::json;
use std::time::{Duration, Instant};

fn errno(c: i32) -> String {
    // WASI errno numbers, as the guest's standard library reports them.
    let name = match -c {
        2 => "EACCES", 8 => "EBADF", 44 => "ENOENT", 52 => "ENOSYS", 58 => "ENOTSUP", 63 => "EPERM", 76 => "ENOTCAPABLE", 1001 => "unsupported", 1002 => "not found", 1003 => "permission denied", _ => "",
    };
    if c >= 0 { format!("ok ({c})") } else { format!("denied: {name} ({c})") }
}

fn short(e: &wasmtime::Error) -> String {
    let s = format!("{e}");
    s.lines().next().unwrap_or("").chars().take(90).collect()
}

/// The proof that the host is still fine: run a normal operation plugin and check its answer.
fn host_alive() -> bool {
    let eng = engine(false, false).unwrap();
    let m = load(&eng, &plugin_path("ops.wasm")).unwrap();
    let mut p = instantiate(&eng, &m, &Grants::default()).unwrap();
    let ptr = p.alloc(64).unwrap();
    p.write(ptr, f32_bytes(&[1.0; 16])).unwrap();
    let f = p.func::<(u32, u32, u32), f32>("op_checksum").unwrap();
    f.call(&mut p.store, (ptr, 2, 2)).map(|v| v == 1.0).unwrap_or(false)
}

fn put(p: &mut Plugin, s: &str) -> (u32, u32) {
    let at = p.alloc(s.len()).unwrap();
    p.write(at, s.as_bytes()).unwrap();
    (at, s.len() as u32)
}

fn peak_rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status").unwrap_or_default().lines().find(|l| l.starts_with("VmHWM:")).and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok()).map(|k| k / 1024.0).unwrap_or(0.0)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).cloned();
    let mut report = serde_json::Map::new();
    let dir = std::env::temp_dir().join("auroraw-spike4");
    let granted = dir.join("granted");
    std::fs::create_dir_all(&granted)?;
    std::fs::write(granted.join("hello.txt"), "hi")?;
    std::fs::write(dir.join("secret.txt"), "the host's private file")?;
    let _ = std::fs::remove_file(granted.join("new.txt"));

    println!("== a plugin that fails or misbehaves: what happens to the host\n");
    let eng = engine(true, true)?;
    let module = load(&eng, &plugin_path("hostile.wasm"))?;
    let mut failures = Vec::new();
    let mut record = |name: &str, outcome: String, took: f64, alive: bool| {
        println!("{name:<44} {outcome:<62} {took:>8.1} ms   host alive: {}", if alive { "yes" } else { "NO" });
        failures.push(json!({ "case": name, "outcome": outcome, "ms": took, "host_alive": alive }));
    };

    // A panic.
    {
        let mut p = instantiate(&eng, &module, &Grants::default())?;
        p.store.set_epoch_deadline(u64::MAX / 2); p.store.set_fuel(u64::MAX / 2)?;
        let f = p.func::<(), u32>("do_panic")?;
        let t = Instant::now();
        let r = f.call(&mut p.store, ());
        record("panic", r.map(|v| format!("returned {v}")).unwrap_or_else(|e| format!("trap: {}", short(&e))), ms(t), host_alive());
    }
    // An infinite loop, stopped by the clock (epoch interruption), at three deadlines.
    for deadline_ms in [10u64, 100, 1000] {
        let mut p = instantiate(&eng, &module, &Grants::default())?;
        p.store.set_fuel(u64::MAX / 2)?;
        p.store.set_epoch_deadline(1);
        let e2 = eng.clone();
        let ticker = std::thread::spawn(move || { std::thread::sleep(Duration::from_millis(deadline_ms)); e2.increment_epoch(); });
        let f = p.func::<(), u32>("do_loop")?;
        let t = Instant::now();
        let r = f.call(&mut p.store, ());
        let took = ms(t);
        ticker.join().unwrap();
        record(&format!("infinite loop, stopped by a {deadline_ms} ms timer"), r.map(|v| format!("returned {v}")).unwrap_or_else(|e| format!("interrupted: {}", short(&e))), took, host_alive());
    }
    // An infinite loop, stopped by a fuel budget.
    {
        let mut p = instantiate(&eng, &module, &Grants::default())?;
        p.store.set_epoch_deadline(u64::MAX / 2);
        p.store.set_fuel(300_000_000)?;
        let f = p.func::<(), u32>("do_loop")?;
        let t = Instant::now();
        let r = f.call(&mut p.store, ());
        record("infinite loop, 300 million steps of fuel", r.map(|v| format!("returned {v}")).unwrap_or_else(|e| format!("stopped: {}", short(&e))), ms(t), host_alive());
    }
    // Hoarding memory.
    for (mb, limit) in [(100u32, 256usize << 20), (2000u32, 256usize << 20)] {
        let mut p = instantiate(&eng, &module, &Grants { memory_limit: Some(limit), ..Default::default() })?;
        p.store.set_epoch_deadline(u64::MAX / 2); p.store.set_fuel(u64::MAX / 2)?;
        let f = p.func::<u32, u32>("do_alloc_bomb")?;
        let before = peak_rss_mb();
        let t = Instant::now();
        let r = f.call(&mut p.store, mb);
        let after = peak_rss_mb();
        record(&format!("asks for {mb} MB, limit {} MB", limit >> 20), r.map(|v| format!("got {v} MB")).unwrap_or_else(|e| format!("refused: {}", short(&e))), ms(t), host_alive());
        println!("{:<44} host peak memory {before:.0} -> {after:.0} MB", "");
    }
    // Endless recursion.
    {
        let mut p = instantiate(&eng, &module, &Grants::default())?;
        p.store.set_epoch_deadline(u64::MAX / 2); p.store.set_fuel(u64::MAX / 2)?;
        let f = p.func::<u32, u32>("do_stack_overflow")?;
        let t = Instant::now();
        let r = f.call(&mut p.store, 0);
        record("endless recursion", r.map(|v| format!("returned {v}")).unwrap_or_else(|e| format!("trap: {}", short(&e))), ms(t), host_alive());
    }
    report.insert("failures".into(), json!(failures));

    println!("\n== a plugin that reaches for what it was not given\n");
    let mut perms = Vec::new();
    let mut check = |case: &str, what: &str, code: i32, expect_ok: bool| {
        let ok = (code >= 0) == expect_ok;
        println!("{case:<32} {what:<44} {:<32} {}", errno(code), if ok { "as intended" } else { "UNEXPECTED" });
        perms.push(json!({ "case": case, "action": what, "result": errno(code), "as_intended": ok }));
    };
    let secret = dir.join("secret.txt").to_string_lossy().into_owned();
    let run_case = |grants: Grants, label: &str, check: &mut dyn FnMut(&str, &str, i32, bool)| -> Result<()> {
        let mut p = instantiate(&eng, &module, &grants)?;
        p.store.set_epoch_deadline(u64::MAX / 2); p.store.set_fuel(u64::MAX / 2)?;
        let granted_on = !grants.read_dirs.is_empty();
        let read = p.func::<(u32, u32), i32>("do_read_file")?;
        let list = p.func::<(u32, u32), i32>("do_list_dir")?;
        let write = p.func::<(u32, u32), i32>("do_write_file")?;
        for (what, path, expect_ok) in [("read the host's private file", secret.as_str(), false), ("read /etc/passwd", "/etc/passwd", false), ("read the granted file", "/granted/hello.txt", granted_on), ("read outside by ../", "/granted/../secret.txt", false)] {
            let (a, l) = put(&mut p, path);
            check(label, what, read.call(&mut p.store, (a, l))?, expect_ok);
        }
        let (a, l) = put(&mut p, "/granted");
        check(label, "list the granted folder", list.call(&mut p.store, (a, l))?, granted_on);
        let (a, l) = put(&mut p, "/granted/new.txt");
        check(label, "write a file in the granted folder", write.call(&mut p.store, (a, l))?, false);
        Ok(())
    };
    run_case(Grants::default(), "nothing granted", &mut check)?;
    run_case(Grants { read_dirs: vec![granted.clone()], ..Default::default() }, "one folder, read only", &mut check)?;
    println!("   was new.txt created on the host? {}", granted.join("new.txt").exists());
    report.insert("new_file_created".into(), json!(granted.join("new.txt").exists()));
    {
        let mut p = instantiate(&eng, &module, &Grants::default())?;
        p.store.set_epoch_deadline(u64::MAX / 2); p.store.set_fuel(u64::MAX / 2)?;
        for (what, name, expect_ok) in [("open a network connection", "do_connect", false), ("start a program", "do_spawn_process", false), ("start a thread", "do_spawn_thread", false)] {
            let f = p.func::<(), i32>(name)?;
            check("nothing granted", what, f.call(&mut p.store, ())?, expect_ok);
        }
        let env = p.func::<(), i32>("do_env_count")?.call(&mut p.store, ())?;
        let argc = p.func::<(), i32>("do_args_count")?.call(&mut p.store, ())?;
        let clock = p.func::<(), i64>("do_clock_ms")?.call(&mut p.store, ())?;
        println!("{:<32} {:<44} {env} variables seen; {argc} argument(s); the clock is readable: {}", "nothing granted", "read the environment, arguments, time", clock > 0);
        report.insert("environment_variables_seen".into(), json!(env));
        report.insert("clock_readable".into(), json!(clock > 0));
    }
    report.insert("permissions".into(), json!(perms));
    println!("\nhost still healthy at the end: {}", host_alive());
    if let Some(o) = out { std::fs::write(o, serde_json::to_string_pretty(&report)?)?; }
    Ok(())
}
