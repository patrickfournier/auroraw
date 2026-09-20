//! Generates the dataset and writes the SQLite catalogue.
//! usage: build_db [--photos N] [--db path]
use anyhow::Result;
use catalogue::{INDEXES, SCHEMA, data_dir, dataset, ms};
use rusqlite::{Connection, params};
use std::time::Instant;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let n: usize = get("--photos").and_then(|v| v.parse().ok()).unwrap_or(100_000);
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let path = get("--db").map(std::path::PathBuf::from).unwrap_or_else(|| dir.join("catalogue.db"));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }

    let t = Instant::now();
    let ds = dataset::generate(n, 42);
    let nv: usize = ds.photos.iter().map(|p| p.versions.len()).sum();
    let nk: usize = ds.photos.iter().map(|p| p.keywords.len()).sum();
    println!("generated {n} photos, {nv} versions, {nk} keyword links, {} series, {} keywords in {:.0} ms", ds.series.len(), ds.keywords.len(), ms(t.elapsed()));

    let mut db = Connection::open(&path)?;
    db.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA locking_mode=EXCLUSIVE;")?;
    db.execute_batch(SCHEMA)?;
    let t = Instant::now();
    let tx = db.transaction()?;
    for (i, c) in ds.cameras.iter().enumerate() {
        tx.execute("INSERT INTO camera VALUES (?,?)", params![i as i64 + 1, c])?;
    }
    for (i, l) in ds.lenses.iter().enumerate() {
        tx.execute("INSERT INTO lens VALUES (?,?)", params![i as i64 + 1, l])?;
    }
    for s in 1..=3 {
        tx.execute("INSERT INTO source VALUES (?,?)", params![s, format!("/archive/source{s}")])?;
    }
    for k in &ds.keywords {
        tx.execute("INSERT INTO keyword(id,parent_id,name,path) VALUES (?,?,?,?)", params![k.id, k.parent, k.name, k.path])?;
    }
    for (id, cover) in &ds.series {
        tx.execute("INSERT INTO series(id,cover_photo_id) VALUES (?,?)", params![id, cover])?;
    }
    {
        let mut ip = tx.prepare("INSERT INTO photo VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")?;
        let mut iv = tx.prepare("INSERT INTO version VALUES (?,?,?,?,?,?,?)")?;
        let mut ik = tx.prepare("INSERT INTO photo_keyword VALUES (?,?)")?;
        let mut ivk = tx.prepare("INSERT INTO version_keyword VALUES (?,?)")?;
        let mut fts = tx.prepare("INSERT INTO photo_fts(rowid, filename, caption, keywords) VALUES (?,?,?,?)")?;
        for p in &ds.photos {
            ip.execute(params![
                p.id, p.source_id, p.path, p.filename, &p.fingerprint[..], p.capture_time, p.camera_id, p.lens_id, p.iso, p.aperture, p.shutter, p.focal, p.width, p.height,
                p.rating, p.flag, p.label, p.caption, p.gps.map(|g| g.0), p.gps.map(|g| g.1), p.series_id, p.stack_visible as i64, p.main_version_id, p.versions.len() as i64, p.effective_rating
            ])?;
            for v in &p.versions {
                iv.execute(params![v.id, v.photo_id, v.name, v.rating, v.flag, v.label, v.updated])?;
                for k in &v.extra_keywords {
                    ivk.execute(params![v.id, k])?;
                }
            }
            for k in &p.keywords {
                ik.execute(params![p.id, k])?;
            }
            let names: Vec<&str> = p.keywords.iter().map(|k| ds.keywords[*k as usize - 1].name.as_str()).collect();
            fts.execute(params![p.id, p.filename, p.caption.as_deref().unwrap_or(""), names.join(" ")])?;
        }
        tx.execute("UPDATE photo SET effective_rating = effective_rating", [])?;
        let mut ic = tx.prepare("INSERT INTO collection VALUES (?,?,0)")?;
        let mut icp = tx.prepare("INSERT INTO collection_photo VALUES (?,?)")?;
        for (id, name, ids) in &ds.collections {
            ic.execute(params![id, name])?;
            for pid in ids {
                icp.execute(params![id, pid])?;
            }
        }
    }
    tx.commit()?;
    println!("rows inserted in {:.0} ms", ms(t.elapsed()));
    let t = Instant::now();
    db.execute_batch(INDEXES)?;
    println!("indexes built in {:.0} ms", ms(t.elapsed()));
    let t = Instant::now();
    db.execute_batch("ANALYZE; PRAGMA optimize;")?;
    println!("analyze in {:.0} ms", ms(t.elapsed()));
    db.execute_batch("PRAGMA journal_mode=WAL;")?;
    drop(db);
    let size = std::fs::metadata(&path)?.len();
    println!("catalogue: {} ({:.0} MB)", path.display(), size as f64 / 1048576.0);
    Ok(())
}
