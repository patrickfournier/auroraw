// SPDX-License-Identifier: GPL-3.0-or-later
//! A measurement, not a check: what each step of an atomic write costs on this file system, alone
//! and under load (design note 001 §4.1, the Windows finding of the large-workspace run).
//!
//! `cargo test -p auroraw-workspace --release --test write_costs -- --ignored --nocapture`

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use auroraw_testkit::temp_dir;

#[derive(Default)]
struct Costs {
    read_miss: Duration,
    mkdir: Duration,
    write_tmp: Duration,
    rename: Duration,
    plain_write: Duration,
}

fn run(threads: usize, per_thread: usize) -> Costs {
    let dir = temp_dir();
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("tmp")).unwrap();
    let totals = std::sync::Mutex::new(Costs::default());
    let body = vec![b'x'; 3000];
    std::thread::scope(|s| {
        for t in 0..threads {
            let (root, totals, body) = (&root, &totals, &body);
            s.spawn(move || {
                let mut c = Costs::default();
                for i in 0..per_thread {
                    let shard = root
                        .join("photos")
                        .join(format!("{:02x}", (t * 31 + i) % 256));
                    let target: PathBuf = shard.join(format!("{t}-{i}.xmp"));
                    let started = Instant::now();
                    let _ = fs::read(&target);
                    c.read_miss += started.elapsed();
                    let started = Instant::now();
                    fs::create_dir_all(&shard).unwrap();
                    c.mkdir += started.elapsed();
                    let tmp = root.join("tmp").join(format!("{t}-{i}.tmp"));
                    let started = Instant::now();
                    let mut f = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&tmp)
                        .unwrap();
                    f.write_all(body).unwrap();
                    drop(f);
                    c.write_tmp += started.elapsed();
                    let started = Instant::now();
                    fs::rename(&tmp, &target).unwrap();
                    c.rename += started.elapsed();
                    let started = Instant::now();
                    fs::write(shard.join(format!("{t}-{i}.plain")), body).unwrap();
                    c.plain_write += started.elapsed();
                }
                let mut total = totals.lock().unwrap();
                total.read_miss += c.read_miss;
                total.mkdir += c.mkdir;
                total.write_tmp += c.write_tmp;
                total.rename += c.rename;
                total.plain_write += c.plain_write;
            });
        }
    });
    totals.into_inner().unwrap()
}

#[test]
#[ignore = "a measurement, run on demand"]
fn what_each_step_of_an_atomic_write_costs() {
    for threads in [1usize, 4] {
        let per_thread = 3000 / threads;
        let n = (per_thread * threads) as f64;
        let c = run(threads, per_thread);
        let ms = |d: Duration| d.as_secs_f64() * 1e3 / n;
        println!(
            "{{\"threads\": {threads}, \"files\": {}, \"per_file_ms\": {{\"read_miss\": {:.3}, \"create_dir_all\": {:.3}, \"write_tmp\": {:.3}, \"rename\": {:.3}, \"plain_write\": {:.3}}}}}",
            n as usize,
            ms(c.read_miss),
            ms(c.mkdir),
            ms(c.write_tmp),
            ms(c.rename),
            ms(c.plain_write)
        );
    }
}
