// Benchmark of workspace layouts (docs/design/001-workspace-layout.md): writes, walks and reads
// 100,000 photo sidecars and their version sidecars in four directory layouts.
// Build and run: rustc -O tools/layout-bench.rs -o layout-bench && ./layout-bench shard256 100000
// (layouts: flat, shard256, shard4096, perphoto). Std only, no dependencies.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

fn hex128(r: &mut Rng) -> String { format!("{:016x}{:016x}", r.next(), r.next()) }

struct Item { dir: PathBuf, name: String, len: usize }

fn build(layout: &str, root: &Path, n: usize) -> Vec<Item> {
    let mut r = Rng(42);
    let mut items = Vec::new();
    for _ in 0..n {
        let id = hex128(&mut r);
        let nv = match r.next() % 100 { 0..=29 => 0, 30..=79 => 1, 80..=89 => 2, _ => 3 };
        let shard2 = &id[..2]; let shard3 = &id[..3];
        match layout {
            "flat" => {
                items.push(Item { dir: root.join("photos"), name: format!("{id}.xmp"), len: 1600 });
                for _ in 0..nv { items.push(Item { dir: root.join("versions"), name: format!("{id}.{:016x}.xmp", r.next()), len: 3300 }); }
            }
            "shard256" | "shard4096" => {
                let s = if layout == "shard256" { shard2 } else { shard3 };
                items.push(Item { dir: root.join("photos").join(s), name: format!("{id}.xmp"), len: 1600 });
                for _ in 0..nv { items.push(Item { dir: root.join("versions").join(s), name: format!("{id}.{:016x}.xmp", r.next()), len: 3300 }); }
            }
            "perphoto" => {
                let d = root.join("photos").join(shard2).join(&id);
                items.push(Item { dir: d.clone(), name: "photo.xmp".into(), len: 1600 });
                for _ in 0..nv { items.push(Item { dir: d.clone(), name: format!("{:016x}.xmp", r.next()), len: 3300 }); }
            }
            _ => panic!(),
        }
    }
    items
}

fn par<T: Sync>(items: &[T], f: impl Fn(&T) + Sync) {
    let threads = 16;
    let chunk = items.len().div_ceil(threads);
    std::thread::scope(|s| { for c in items.chunks(chunk) { let f = &f; s.spawn(move || c.iter().for_each(f)); } });
}

fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64)>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        let m = e.metadata().unwrap();
        if m.is_dir() { walk(&p, out) } else { out.push((p, m.len())) }
    }
}

fn main() {
    let layout = std::env::args().nth(1).unwrap();
    let n: usize = std::env::args().nth(2).unwrap().parse().unwrap();
    let root = std::env::temp_dir().join("auroraw-layout-bench").join(&layout);
    let _ = fs::remove_dir_all(&root);
    let items = build(&layout, &root, n);
    let t = Instant::now();
    let mut dirs: Vec<&PathBuf> = items.iter().map(|i| &i.dir).collect();
    dirs.sort(); dirs.dedup();
    let ndirs = dirs.len();
    par(&dirs, |d| { fs::create_dir_all(d).unwrap(); });
    let t_dirs = t.elapsed();
    let payload = vec![b'x'; 4000];
    let t = Instant::now();
    par(&items, |i| { fs::write(i.dir.join(&i.name), &payload[..i.len]).unwrap(); });
    let t_write = t.elapsed();
    let t = Instant::now();
    let mut files = Vec::new();
    walk(&root, &mut files);
    let t_walk = t.elapsed();
    let t = Instant::now();
    let total = std::sync::atomic::AtomicU64::new(0);
    par(&files, |(p, _)| { let b = fs::read(p).unwrap(); total.fetch_add(b.len() as u64, std::sync::atomic::Ordering::Relaxed); });
    let t_read = t.elapsed();
    // one atomic rewrite (temp then rename), median of 300 random files
    let mut r = Rng(7);
    let mut times = Vec::new();
    for _ in 0..300 {
        let i = &items[(r.next() % items.len() as u64) as usize];
        let target = i.dir.join(&i.name);
        let tmp = root.join("tmp-write");
        let t = Instant::now();
        fs::write(&tmp, &payload[..i.len]).unwrap();
        fs::rename(&tmp, &target).unwrap();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("{layout:9} files {:>7} dirs {:>7} | mkdirs {:>6.0} ms | write {:>6.0} ms | walk {:>6.0} ms | read {:>6.0} ms | rewrite med {:.3} ms",
        files.len(), ndirs, t_dirs.as_secs_f64()*1000.0, t_write.as_secs_f64()*1000.0, t_walk.as_secs_f64()*1000.0, t_read.as_secs_f64()*1000.0, times[150]);
    fs::remove_dir_all(&root).unwrap();
}
