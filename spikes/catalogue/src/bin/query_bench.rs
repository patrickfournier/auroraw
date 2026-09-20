//! Times the queries a catalogue answers all day, on the 100,000-photo database.
//! usage: query_bench [--db path] [--out result.json]
use anyhow::Result;
use catalogue::{data_dir, dataset, evict, ms, pct};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::{Value, json};
use std::time::Instant;

const COLS: &str = "p.id, p.filename, p.capture_time, p.effective_rating, p.flag, p.label, p.version_count, p.series_id";

fn timed(iters: usize, mut f: impl FnMut(usize) -> rusqlite::Result<usize>) -> (Vec<f64>, usize) {
    let mut times = Vec::new();
    let mut rows = 0;
    for i in 0..iters {
        let t = Instant::now();
        rows = f(i).unwrap();
        times.push(ms(t.elapsed()));
    }
    (times, rows)
}

fn page(db: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> rusqlite::Result<usize> {
    let mut st = db.prepare_cached(sql)?;
    let mut rows = st.query(p)?;
    let mut n = 0;
    while let Some(r) = rows.next()? {
        let _: i64 = r.get(0)?;
        let _: String = r.get(1)?;
        n += 1;
    }
    Ok(n)
}

fn count(db: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> rusqlite::Result<usize> {
    db.prepare_cached(sql)?.query_row(p, |r| r.get::<_, i64>(0)).map(|v| v as usize)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let path = get("--db").map(std::path::PathBuf::from).unwrap_or_else(|| data_dir().join("catalogue.db"));
    let mut results = serde_json::Map::new();

    // 1. Opening the catalogue cold: the file is dropped from the page cache first.
    let mut open_ms = Vec::new();
    for _ in 0..5 {
        for ext in ["", "-wal", "-shm"] {
            evict(std::path::Path::new(&format!("{}{ext}", path.display())));
        }
        let t = Instant::now();
        let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        let n = count(&db, "SELECT count(*) FROM photo p WHERE stack_visible=1", &[])?;
        let first = page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 ORDER BY capture_time DESC LIMIT 200"), &[])?;
        assert!(n > 0 && first == 200);
        open_ms.push(ms(t.elapsed()));
    }
    println!("open cold (open, count, first page of 200): median {:.1} ms, worst {:.1} ms", pct(&open_ms, 0.5), pct(&open_ms, 1.0));
    results.insert("open_cold_ms".into(), json!({ "median": pct(&open_ms, 0.5), "worst": pct(&open_ms, 1.0) }));

    let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    db.execute_batch("PRAGMA synchronous=NORMAL; PRAGMA cache_size=-65536; PRAGMA mmap_size=268435456;")?;
    let total: i64 = db.query_row("SELECT count(*) FROM photo", [], |r| r.get(0))?;
    let (tmin, tmax): (i64, i64) = db.query_row("SELECT min(capture_time), max(capture_time) FROM photo", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let groups: Vec<String> = db.prepare("SELECT path FROM keyword WHERE parent_id IS NOT NULL AND path NOT LIKE '%/%/%'")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let cats: Vec<String> = db.prepare("SELECT path FROM keyword WHERE parent_id IS NULL")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let range = |p: &str| (format!("{p}/"), format!("{p}0"));
    let mut rng = fastrand::Rng::with_seed(7);
    let month = (tmax - tmin) / 130;

    let mut report = |name: &str, note: &str, (times, rows): (Vec<f64>, usize)| {
        println!("{name:<44} median {:>7.2} ms  p95 {:>7.2}  worst {:>7.2}  rows {rows:>6}  {note}", pct(&times, 0.5), pct(&times, 0.95), pct(&times, 1.0));
        results.insert(name.into(), json!({ "median_ms": pct(&times, 0.5), "p95_ms": pct(&times, 0.95), "worst_ms": pct(&times, 1.0), "rows": rows, "note": note }));
    };

    println!("\n-- the grid page (200 photos)");
    report("newest 200 visible", "", timed(30, |_| page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 ORDER BY capture_time DESC LIMIT 200"), &[])));
    report("count of visible photos", "", timed(30, |_| count(&db, "SELECT count(*) FROM photo p WHERE stack_visible=1", &[])));
    let starts: Vec<i64> = (0..30).map(|_| tmin + rng.i64(0..(tmax - tmin))).collect();
    report("keyset page at a random date", "capture_time < ?", timed(30, |i| page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 AND capture_time < ?1 ORDER BY capture_time DESC LIMIT 200"), &[&starts[i]])));
    report("OFFSET 80,000 page", "the slow way", timed(10, |_| page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 ORDER BY capture_time DESC LIMIT 200 OFFSET 80000"), &[])));

    println!("\n-- filters");
    report("rating >= 4, stored effective rating", "page", timed(30, |_| page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 AND effective_rating>=4 ORDER BY capture_time DESC LIMIT 200"), &[])));
    report("rating >= 4, stored, count", "", timed(30, |_| count(&db, "SELECT count(*) FROM photo WHERE stack_visible=1 AND effective_rating>=4", &[])));
    report("rating >= 4, computed by join", "page", timed(30, |_| page(&db, &format!("SELECT {COLS} FROM photo p LEFT JOIN version v ON v.id=p.main_version_id WHERE p.stack_visible=1 AND COALESCE(v.rating,p.rating)>=4 ORDER BY p.capture_time DESC LIMIT 200"), &[])));
    report("rating >= 4, computed by join, count", "", timed(30, |_| count(&db, "SELECT count(*) FROM photo p LEFT JOIN version v ON v.id=p.main_version_id WHERE p.stack_visible=1 AND COALESCE(v.rating,p.rating)>=4", &[])));
    let months: Vec<i64> = (0..30).map(|_| tmin + rng.i64(0..(tmax - tmin - month))).collect();
    report("one month of shooting", "date range", timed(30, |i| page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 AND capture_time BETWEEN ?1 AND ?2 ORDER BY capture_time LIMIT 200"), &[&months[i], &(months[i] + month)])));
    let cam: Vec<i64> = (0..30).map(|i| 1 + (i % 12) as i64).collect();
    report("camera + ISO 800-3200", "page", timed(30, |i| page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 AND camera_id=?1 AND iso BETWEEN 800 AND 3200 ORDER BY capture_time DESC LIMIT 200"), &[&cam[i]])));
    report("camera + ISO 800-3200, count", "", timed(30, |i| count(&db, "SELECT count(*) FROM photo WHERE stack_visible=1 AND camera_id=?1 AND iso BETWEEN 800 AND 3200", &[&cam[i]])));

    println!("\n-- keywords (hierarchical: a group is 10 keywords, a category 200)");
    let kwsql = "p.id IN (SELECT pk.photo_id FROM photo_keyword pk JOIN keyword k ON k.id=pk.keyword_id WHERE k.path >= ?1 AND k.path < ?2)";
    let gr: Vec<(String, String)> = (0..30).map(|_| range(&groups[rng.usize(0..groups.len())])).collect();
    report("keyword group, page", "a group of 10 keywords", timed(30, |i| page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 AND {kwsql} ORDER BY p.capture_time DESC LIMIT 200"), &[&gr[i].0, &gr[i].1])));
    report("keyword group, count", "", timed(30, |i| count(&db, &format!("SELECT count(*) FROM photo p WHERE stack_visible=1 AND {kwsql}"), &[&gr[i].0, &gr[i].1])));
    let ca: Vec<(String, String)> = (0..20).map(|_| range(&cats[rng.usize(0..cats.len())])).collect();
    report("keyword category, page", "200 keywords", timed(20, |i| page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 AND {kwsql} ORDER BY p.capture_time DESC LIMIT 200"), &[&ca[i].0, &ca[i].1])));
    report("keyword category, count", "", timed(20, |i| count(&db, &format!("SELECT count(*) FROM photo p WHERE stack_visible=1 AND {kwsql}"), &[&ca[i].0, &ca[i].1])));
    report("category + rating >= 3 + a year", "combined", timed(20, |i| page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 AND effective_rating>=3 AND capture_time BETWEEN ?3 AND ?4 AND {kwsql} ORDER BY p.capture_time DESC LIMIT 200"), &[&ca[i].0, &ca[i].1, &months[i], &(months[i] + month * 12)])));

    println!("\n-- text search");
    let words: Vec<String> = (0..30).map(|_| dataset::word(rng.usize(0..dataset::VOCAB))).collect();
    report("full-text search (prefix), page", "FTS5", timed(30, |i| page(&db, &format!("SELECT {COLS} FROM photo_fts f JOIN photo p ON p.id=f.rowid WHERE photo_fts MATCH ?1 AND p.stack_visible=1 ORDER BY p.capture_time DESC LIMIT 200"), &[&format!("{}*", words[i])])));
    report("full-text search (prefix), count", "", timed(30, |i| count(&db, "SELECT count(*) FROM photo_fts f JOIN photo p ON p.id=f.rowid WHERE photo_fts MATCH ?1 AND p.stack_visible=1", &[&format!("{}*", words[i])])));
    report("LIKE '%word%' on caption and filename", "no index", timed(10, |i| page(&db, &format!("SELECT {COLS} FROM photo p WHERE stack_visible=1 AND (caption LIKE ?1 OR filename LIKE ?1) ORDER BY capture_time DESC LIMIT 200"), &[&format!("%{}%", words[i])])));

    println!("\n-- collections, series and facets");
    report("a collection (20 to 2,000 photos)", "", timed(30, |i| page(&db, &format!("SELECT {COLS} FROM collection_photo cp JOIN photo p ON p.id=cp.photo_id WHERE cp.collection_id=?1 AND p.stack_visible=1 ORDER BY p.capture_time DESC LIMIT 200"), &[&(1 + i as i64 * 5)])));
    report("expand a series", "", timed(30, |_| { let sid: i64 = 1 + rng.i64(0..2000); page(&db, &format!("SELECT {COLS} FROM photo p WHERE series_id=?1 ORDER BY capture_time"), &[&sid]) }));
    report("facet: ratings under a keyword category", "GROUP BY", timed(20, |i| { let mut st = db.prepare_cached(&format!("SELECT effective_rating, count(*) FROM photo p WHERE stack_visible=1 AND {kwsql} GROUP BY 1"))?; let n = st.query_map(params![ca[i].0, ca[i].1], |r| r.get::<_, i64>(0))?.count(); Ok(n) }));
    report("facet: top 20 keywords under a category", "the heavy one", timed(10, |i| { let mut st = db.prepare_cached(&format!("SELECT pk2.keyword_id, count(*) c FROM photo_keyword pk2 WHERE pk2.photo_id IN (SELECT p.id FROM photo p WHERE p.stack_visible=1 AND {kwsql}) GROUP BY 1 ORDER BY c DESC LIMIT 20"))?; let n = st.query_map(params![ca[i].0, ca[i].1], |r| r.get::<_, i64>(0))?.count(); Ok(n) }));

    println!("\n-- writes (1,000 photos at a time)");
    for mode in ["NORMAL", "FULL"] {
        db.execute_batch(&format!("PRAGMA synchronous={mode};"))?;
        let ids: Vec<i64> = (0..1000).map(|_| 1 + rng.i64(0..total)).collect();
        let (t, rows) = timed(5, |_| {
            let tx = db.unchecked_transaction()?;
            {
                let mut st = tx.prepare_cached("UPDATE photo SET rating=?1, effective_rating=?1 WHERE id=?2")?;
                for id in &ids {
                    st.execute(params![rng.i64(0..6), id])?;
                }
            }
            tx.commit()?;
            Ok(ids.len())
        });
        report(&format!("set the rating of 1,000 photos (sync {mode})"), "one transaction", (t, rows));
        let (t, _) = timed(200, |_| { db.execute("UPDATE photo SET rating=3, effective_rating=3 WHERE id=?1", params![1 + rng.i64(0..total)])?; Ok(1) });
        report(&format!("set the rating of one photo (sync {mode})"), "one transaction each", (t, 1));
        let (t, rows) = timed(5, |_| {
            let tx = db.unchecked_transaction()?;
            {
                let mut st = tx.prepare_cached("INSERT OR IGNORE INTO photo_keyword VALUES (?1, ?2)")?;
                for id in &ids {
                    st.execute(params![id, 1 + rng.i64(0..4000)])?;
                }
            }
            tx.commit()?;
            Ok(ids.len())
        });
        report(&format!("add a keyword to 1,000 photos (sync {mode})"), "", (t, rows));
    }

    let size = std::fs::metadata(&path)?.len();
    results.insert("db_size_mb".into(), Value::from(size as f64 / 1048576.0));
    if let Some(out) = get("--out") {
        std::fs::write(out, serde_json::to_string_pretty(&Value::Object(results))?)?;
    }
    Ok(())
}
