//! The workspace of sidecars: writing 100,000 photo sidecars and 143,000 version sidecars, then
//! rebuilding the catalogue from them (D-026).
//! usage: workspace --write | --rebuild [--cold] | --edit   [--out result.json]
use anyhow::Result;
use catalogue::{INDEXES, SCHEMA, data_dir, dataset, evict, ms, pct};
use quick_xml::Reader;
use quick_xml::events::Event;
use rayon::prelude::*;
use rusqlite::{Connection, params};
use serde_json::json;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn photo_path(root: &Path, id: i64) -> PathBuf { root.join("photos").join(format!("{:03}", id / 1000)).join(format!("{id}.xmp")) }
fn version_path(root: &Path, id: i64) -> PathBuf { root.join("versions").join(format!("{:03}", id / 1000)).join(format!("{id}.xmp")) }

const HEAD: &str = "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n";
const NS: &str = "xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:lr=\"http://ns.adobe.com/lightroom/1.0/\" xmlns:exif=\"http://ns.adobe.com/exif/1.0/\" xmlns:auroraw=\"https://auroraw.org/ns/1.0/\"";

fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }

/// The photo sidecar: XMP with the photo's metadata, and a cache of what was read from the
/// original (capture time, camera, exposure) so a rebuild need not open the originals.
fn photo_xmp(p: &dataset::PhotoRow, ds: &dataset::Dataset) -> String {
    let mut s = String::with_capacity(2800);
    s.push_str(HEAD);
    let _ = write!(s, "  <rdf:Description rdf:about=\"\" {NS}\n    xmp:Rating=\"{}\" auroraw:Flag=\"{}\" auroraw:Label=\"{}\" auroraw:PhotoId=\"{}\" auroraw:Fingerprint=\"{}\" auroraw:MainVersion=\"{}\"", p.rating, p.flag, p.label, p.id, hex(&p.fingerprint), p.main_version_id);
    if let Some(sid) = p.series_id { let _ = write!(s, " auroraw:SeriesId=\"{sid}\" auroraw:StackVisible=\"{}\"", p.stack_visible as i32); }
    let _ = write!(s, "\n    auroraw:Path=\"{}\" auroraw:SourceId=\"{}\" exif:DateTimeOriginal=\"{}\" auroraw:CameraId=\"{}\" auroraw:LensId=\"{}\" exif:ISOSpeedRatings=\"{}\" exif:FNumber=\"{}\" exif:ExposureTime=\"{}\" exif:FocalLength=\"{}\" exif:PixelXDimension=\"{}\" exif:PixelYDimension=\"{}\"", p.path, p.source_id, p.capture_time, p.camera_id, p.lens_id, p.iso, p.aperture, p.shutter, p.focal, p.width, p.height);
    if let Some((la, lo)) = p.gps { let _ = write!(s, " exif:GPSLatitude=\"{la:.5}\" exif:GPSLongitude=\"{lo:.5}\""); }
    s.push_str(">\n");
    if let Some(c) = &p.caption { let _ = writeln!(s, "   <dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{c}</rdf:li></rdf:Alt></dc:description>"); }
    s.push_str("   <dc:subject><rdf:Bag>\n");
    for k in &p.keywords { let _ = writeln!(s, "    <rdf:li>{}</rdf:li>", ds.keywords[*k as usize - 1].name); }
    s.push_str("   </rdf:Bag></dc:subject>\n   <lr:hierarchicalSubject><rdf:Bag>\n");
    for k in &p.keywords { let _ = writeln!(s, "    <rdf:li>{}</rdf:li>", ds.keywords[*k as usize - 1].path.replace('/', "|")); }
    s.push_str("   </rdf:Bag></lr:hierarchicalSubject>\n  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>\n");
    s
}

/// The version sidecar: the effective values, its overrides, and its history as JSON.
fn version_xmp(v: &dataset::VersionRow, p: &dataset::PhotoRow, ds: &dataset::Dataset, rng: &mut fastrand::Rng) -> String {
    let mut s = String::with_capacity(9000);
    s.push_str(HEAD);
    let _ = write!(s, "  <rdf:Description rdf:about=\"\" {NS}\n    auroraw:VersionId=\"{}\" auroraw:PhotoId=\"{}\" auroraw:Name=\"{}\" auroraw:Updated=\"{}\" xmp:Rating=\"{}\"", v.id, v.photo_id, v.name, v.updated, v.rating.unwrap_or(p.rating));
    if let Some(r) = v.rating { let _ = write!(s, " auroraw:RatingOverride=\"{r}\""); }
    s.push_str(">\n   <lr:hierarchicalSubject><rdf:Bag>\n");
    for k in v.extra_keywords.iter().chain(p.keywords.iter()) { let _ = writeln!(s, "    <rdf:li>{}</rdf:li>", ds.keywords[*k as usize - 1].path.replace('/', "|")); }
    s.push_str("   </rdf:Bag></lr:hierarchicalSubject>\n   <auroraw:History>{\"schema\":1,\"pipeline\":\"default-1\",\"ops\":[");
    let ops = ["exposure", "whitebalance", "tone", "contrast", "vibrance", "hsl", "sharpen", "denoise", "crop", "localadjust", "grain", "vignette"];
    for i in 0..rng.usize(18..40) {
        if i > 0 { s.push(','); }
        let _ = write!(s, "{{\"op\":\"{}\",\"on\":true,\"v\":1,\"p\":[{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}]}}", ops[i % ops.len()], rng.f32(), rng.f32(), rng.f32(), rng.f32(), rng.f32(), rng.f32());
    }
    s.push_str("],\"snapshots\":[");
    for i in 0..rng.usize(0..4) { if i > 0 { s.push(','); } let _ = write!(s, "{{\"name\":\"Snapshot {i}\",\"at\":{}}}", rng.usize(1..30)); }
    s.push_str("]}</auroraw:History>\n  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>\n");
    s
}

#[derive(Default, Debug)]
struct PhotoRec { id: i64, rating: i64, flag: i64, label: i64, fp: Vec<u8>, main_version: i64, series: Option<i64>, stack_visible: bool, path: String, source: i64, capture: i64, camera: i64, lens: i64, iso: i64, aperture: f64, shutter: f64, focal: f64, width: i64, height: i64, gps: Option<(f64, f64)>, caption: Option<String>, keywords: Vec<String> }
#[derive(Default)]
struct VerRec { id: i64, photo: i64, name: String, updated: i64, rating: Option<i64>, extra: Vec<String>, history_bytes: usize }

fn attr(a: &quick_xml::events::attributes::Attribute) -> (String, String) {
    (a.key.as_ref().to_string(), a.unescape_value().map(|v| v.into_owned()).unwrap_or_default())
}

fn parse_photo(data: &[u8]) -> PhotoRec {
    let mut r = Reader::from_reader(data);
    let mut rec = PhotoRec::default();
    let (mut buf, mut in_hier, mut in_desc, mut text) = (Vec::new(), false, false, false);
    loop {
        match r.read_event_into(&mut buf).unwrap() {
            Event::Start(e) | Event::Empty(e) => {
                match e.name().as_ref() {
                    "rdf:Description" => for a in e.attributes().flatten() {
                        let (k, v) = attr(&a);
                        match k.as_str() {
                            "xmp:Rating" => rec.rating = v.parse().unwrap(), "auroraw:Flag" => rec.flag = v.parse().unwrap(), "auroraw:Label" => rec.label = v.parse().unwrap(),
                            "auroraw:PhotoId" => rec.id = v.parse().unwrap(), "auroraw:Fingerprint" => rec.fp = (0..v.len() / 2).map(|i| u8::from_str_radix(&v[2 * i..2 * i + 2], 16).unwrap()).collect(),
                            "auroraw:MainVersion" => rec.main_version = v.parse().unwrap(), "auroraw:SeriesId" => rec.series = v.parse().ok(), "auroraw:StackVisible" => rec.stack_visible = v == "1",
                            "auroraw:Path" => rec.path = v, "auroraw:SourceId" => rec.source = v.parse().unwrap(), "exif:DateTimeOriginal" => rec.capture = v.parse().unwrap(),
                            "auroraw:CameraId" => rec.camera = v.parse().unwrap(), "auroraw:LensId" => rec.lens = v.parse().unwrap(), "exif:ISOSpeedRatings" => rec.iso = v.parse().unwrap(),
                            "exif:FNumber" => rec.aperture = v.parse().unwrap(), "exif:ExposureTime" => rec.shutter = v.parse().unwrap(), "exif:FocalLength" => rec.focal = v.parse().unwrap(),
                            "exif:PixelXDimension" => rec.width = v.parse().unwrap(), "exif:PixelYDimension" => rec.height = v.parse().unwrap(),
                            "exif:GPSLatitude" => rec.gps = Some((v.parse().unwrap(), rec.gps.map_or(0.0, |g| g.1))), "exif:GPSLongitude" => rec.gps = Some((rec.gps.map_or(0.0, |g| g.0), v.parse().unwrap())),
                            _ => {}
                        }
                    },
                    "lr:hierarchicalSubject" => in_hier = true,
                    "dc:description" => in_desc = true,
                    "rdf:li" => text = true,
                    _ => {}
                }
            }
            Event::Text(t) if text => {
                let v = t.into_inner().into_owned();
                if in_hier { rec.keywords.push(v); } else if in_desc { rec.caption = Some(v); }
            }
            Event::End(e) => match e.name().as_ref() { "lr:hierarchicalSubject" => in_hier = false, "dc:description" => in_desc = false, "rdf:li" => text = false, _ => {} },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    rec
}

fn parse_version(data: &[u8]) -> VerRec {
    let mut r = Reader::from_reader(data);
    let mut rec = VerRec::default();
    let (mut buf, mut in_hier, mut text, mut in_hist) = (Vec::new(), false, false, false);
    loop {
        match r.read_event_into(&mut buf).unwrap() {
            Event::Start(e) | Event::Empty(e) => match e.name().as_ref() {
                "rdf:Description" => for a in e.attributes().flatten() {
                    let (k, v) = attr(&a);
                    match k.as_str() { "auroraw:VersionId" => rec.id = v.parse().unwrap(), "auroraw:PhotoId" => rec.photo = v.parse().unwrap(), "auroraw:Name" => rec.name = v, "auroraw:Updated" => rec.updated = v.parse().unwrap(), "auroraw:RatingOverride" => rec.rating = v.parse().ok(), _ => {} }
                },
                "lr:hierarchicalSubject" => in_hier = true,
                "rdf:li" => text = true,
                "auroraw:History" => in_hist = true,
                _ => {}
            },
            Event::Text(t) => { if in_hist { rec.history_bytes += t.len(); } else if text && in_hier { rec.extra.push(t.into_inner().into_owned()); } }
            Event::End(e) => match e.name().as_ref() { "lr:hierarchicalSubject" => in_hier = false, "rdf:li" => text = false, "auroraw:History" => in_hist = false, _ => {} },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    rec
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for sub in std::fs::read_dir(dir).unwrap().flatten() {
        for f in std::fs::read_dir(sub.path()).unwrap().flatten() { v.push(f.path()); }
    }
    v
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let get = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let root = data_dir().join("workspace");
    let mut out = serde_json::Map::new();
    let ds = dataset::generate(100_000, 42);

    if args.iter().any(|a| a == "--write") {
        let _ = std::fs::remove_dir_all(&root);
        let t = Instant::now();
        (0..=150).into_par_iter().for_each(|d| { std::fs::create_dir_all(root.join("photos").join(format!("{d:03}"))).unwrap(); std::fs::create_dir_all(root.join("versions").join(format!("{d:03}"))).unwrap(); });
        std::fs::create_dir_all(root.join("state"))?;
        let t_photos = Instant::now();
        let sizes: Vec<usize> = ds.photos.par_iter().map(|p| { let x = photo_xmp(p, &ds); std::fs::write(photo_path(&root, p.id), &x).unwrap(); x.len() }).collect();
        let photos_s = t_photos.elapsed().as_secs_f64();
        let t_ver = Instant::now();
        let vsizes: Vec<usize> = ds.photos.par_iter().flat_map_iter(|p| { let mut rng = fastrand::Rng::with_seed(p.id as u64); p.versions.iter().map(|v| { let x = version_xmp(v, p, &ds, &mut rng); std::fs::write(version_path(&root, v.id), &x).unwrap(); x.len() }).collect::<Vec<_>>() }).collect();
        let ver_s = t_ver.elapsed().as_secs_f64();
        // State files: the vocabulary, the collections, the series (D-026).
        let vocab: Vec<_> = ds.keywords.iter().map(|k| json!({ "id": k.id, "parent": k.parent, "name": k.name, "path": k.path })).collect();
        std::fs::write(root.join("state/vocabulary.json"), serde_json::to_vec(&vocab)?)?;
        let cols: Vec<_> = ds.collections.iter().map(|(id, n, ids)| json!({ "id": id, "name": n, "photos": ids })).collect();
        std::fs::write(root.join("state/collections.json"), serde_json::to_vec(&cols)?)?;
        let ser: Vec<_> = ds.series.iter().map(|(id, c)| json!({ "id": id, "cover": c })).collect();
        std::fs::write(root.join("state/series.json"), serde_json::to_vec(&ser)?)?;
        println!("wrote {} photo sidecars ({:.1} KB each) in {photos_s:.1} s and {} version sidecars ({:.1} KB each) in {ver_s:.1} s, total {:.1} s, on 16 threads", sizes.len(), sizes.iter().sum::<usize>() as f64 / sizes.len() as f64 / 1024.0, vsizes.len(), vsizes.iter().sum::<usize>() as f64 / vsizes.len() as f64 / 1024.0, t.elapsed().as_secs_f64());
        out.insert("write".into(), json!({ "photo_sidecars": sizes.len(), "photo_kb": sizes.iter().sum::<usize>() as f64 / sizes.len() as f64 / 1024.0, "version_sidecars": vsizes.len(), "version_kb": vsizes.iter().sum::<usize>() as f64 / vsizes.len() as f64 / 1024.0, "seconds": t.elapsed().as_secs_f64() }));
    }

    if args.iter().any(|a| a == "--edit") {
        // What one edit costs: rewrite a photo sidecar atomically (temporary file, then rename),
        // with and without forcing it to the disk.
        for (name, sync) in [("without fsync", false), ("with fsync", true)] {
            let mut rng = fastrand::Rng::with_seed(3);
            let times: Vec<f64> = (0..500).map(|_| {
                let p = &ds.photos[rng.usize(0..ds.photos.len())];
                let x = photo_xmp(p, &ds);
                let dest = photo_path(&root, p.id);
                let tmp = dest.with_extension("tmp");
                let t = Instant::now();
                { let f = std::fs::File::create(&tmp).unwrap(); use std::io::Write; let mut f = f; f.write_all(x.as_bytes()).unwrap(); if sync { f.sync_all().unwrap(); } }
                std::fs::rename(&tmp, &dest).unwrap();
                ms(t.elapsed())
            }).collect();
            println!("rewrite one photo sidecar {name}: median {:.3} ms, p95 {:.3}, worst {:.3}", pct(&times, 0.5), pct(&times, 0.95), pct(&times, 1.0));
            out.insert(format!("edit_{}", name.replace(' ', "_")), json!({ "median_ms": pct(&times, 0.5), "p95_ms": pct(&times, 0.95), "worst_ms": pct(&times, 1.0) }));
        }
    }

    if args.iter().any(|a| a == "--rebuild") {
        let cold = args.iter().any(|a| a == "--cold");
        let t_all = Instant::now();
        let t = Instant::now();
        let pfiles = walk(&root.join("photos"));
        let vfiles = walk(&root.join("versions"));
        let walk_ms = ms(t.elapsed());
        if cold {
            pfiles.iter().chain(vfiles.iter()).for_each(|p| evict(p));
            for f in ["vocabulary.json", "collections.json", "series.json"] { evict(&root.join("state").join(f)); }
        }
        // Detecting changes: one stat per file.
        let t = Instant::now();
        let n_stat = pfiles.par_iter().chain(vfiles.par_iter()).filter(|p| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)).count();
        let stat_ms = ms(t.elapsed());
        if cold { pfiles.iter().chain(vfiles.iter()).for_each(|p| evict(p)); }

        let t = Instant::now();
        let vocab: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(root.join("state/vocabulary.json"))?)?;
        let cols: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(root.join("state/collections.json"))?)?;
        let sers: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(root.join("state/series.json"))?)?;
        let state_ms = ms(t.elapsed());
        let kw_ids: HashMap<String, i64> = vocab.iter().map(|k| (k["path"].as_str().unwrap().replace('/', "|"), k["id"].as_i64().unwrap())).collect();

        let t = Instant::now();
        let photos: Vec<PhotoRec> = pfiles.par_iter().map(|p| parse_photo(&std::fs::read(p).unwrap())).collect();
        let photos_ms = ms(t.elapsed());
        let t = Instant::now();
        let versions: Vec<VerRec> = vfiles.par_iter().map(|p| parse_version(&std::fs::read(p).unwrap())).collect();
        let versions_ms = ms(t.elapsed());

        let path = data_dir().join("catalogue-rebuilt.db");
        for ext in ["", "-wal", "-shm"] { let _ = std::fs::remove_file(format!("{}{ext}", path.display())); }
        let mut db = Connection::open(&path)?;
        db.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA locking_mode=EXCLUSIVE;")?;
        db.execute_batch(SCHEMA)?;
        let t = Instant::now();
        let tx = db.transaction()?;
        for (i, c) in ds.cameras.iter().enumerate() { tx.execute("INSERT INTO camera VALUES (?,?)", params![i as i64 + 1, c])?; }
        for (i, l) in ds.lenses.iter().enumerate() { tx.execute("INSERT INTO lens VALUES (?,?)", params![i as i64 + 1, l])?; }
        for s in 1..=3 { tx.execute("INSERT INTO source VALUES (?,?)", params![s, format!("/archive/source{s}")])?; }
        for k in &vocab { tx.execute("INSERT INTO keyword(id,parent_id,name,path) VALUES (?,?,?,?)", params![k["id"].as_i64(), k["parent"].as_i64(), k["name"].as_str(), k["path"].as_str()])?; }
        for s in &sers { tx.execute("INSERT INTO series(id,cover_photo_id) VALUES (?,?)", params![s["id"].as_i64(), s["cover"].as_i64()])?; }
        for c in &cols { tx.execute("INSERT INTO collection VALUES (?,?,0)", params![c["id"].as_i64(), c["name"].as_str()])?; for pid in c["photos"].as_array().unwrap() { tx.execute("INSERT INTO collection_photo VALUES (?,?)", params![c["id"].as_i64(), pid.as_i64()])?; } }
        let vcount: HashMap<i64, i64> = versions.iter().fold(HashMap::new(), |mut m, v| { *m.entry(v.photo).or_insert(0) += 1; m });
        let vrate: HashMap<i64, Option<i64>> = versions.iter().map(|v| (v.id, v.rating)).collect();
        {
            let mut ip = tx.prepare("INSERT INTO photo VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")?;
            let mut ik = tx.prepare("INSERT INTO photo_keyword VALUES (?,?)")?;
            let mut fts = tx.prepare("INSERT INTO photo_fts(rowid, filename, caption, keywords) VALUES (?,?,?,?)")?;
            for p in &photos {
                let eff = vrate.get(&p.main_version).copied().flatten().unwrap_or(p.rating);
                let fname = p.path.rsplit('/').next().unwrap_or("").to_string();
                ip.execute(params![p.id, p.source, p.path, fname, &p.fp[..], p.capture, p.camera, p.lens, p.iso, p.aperture, p.shutter, p.focal, p.width, p.height, p.rating, p.flag, p.label, p.caption, p.gps.map(|g| g.0), p.gps.map(|g| g.1), p.series, p.stack_visible.then_some(1).unwrap_or(if p.series.is_none() { 1 } else { 0 }), p.main_version, vcount.get(&p.id).copied().unwrap_or(1), eff])?;
                let mut names = Vec::new();
                for k in &p.keywords { if let Some(id) = kw_ids.get(k) { ik.execute(params![p.id, id])?; names.push(k.rsplit('|').next().unwrap().to_string()); } }
                fts.execute(params![p.id, fname, p.caption.as_deref().unwrap_or(""), names.join(" ")])?;
            }
            let mut iv = tx.prepare("INSERT INTO version VALUES (?,?,?,?,?,?,?)")?;
            for v in &versions { iv.execute(params![v.id, v.photo, v.name, v.rating, Option::<i64>::None, Option::<i64>::None, v.updated])?; }
        }
        tx.commit()?;
        let insert_ms = ms(t.elapsed());
        let t = Instant::now();
        db.execute_batch(INDEXES)?;
        let index_ms = ms(t.elapsed());
        let total_ms = ms(t_all.elapsed());
        // Is the rebuilt catalogue the same as the original?
        let orig = Connection::open_with_flags(data_dir().join("catalogue.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let sums = |c: &Connection| -> Vec<i64> { ["SELECT count(*) FROM photo", "SELECT sum(rating) FROM photo", "SELECT sum(effective_rating) FROM photo", "SELECT sum(capture_time) FROM photo", "SELECT count(*) FROM version", "SELECT count(*) FROM photo_keyword", "SELECT sum(stack_visible) FROM photo", "SELECT count(*) FROM collection_photo", "SELECT sum(iso) FROM photo"].iter().map(|q| c.query_row(q, [], |r| r.get::<_, i64>(0)).unwrap()).collect() };
        let (a, b) = (sums(&orig), sums(&db));
        // The original catalogue got its edits from the query benchmark (ratings changed); compare the structure only.
        let same_structure = a[0] == b[0] && a[3] == b[3] && a[4] == b[4] && a[5] >= b[5] && a[7] == b[7] && a[8] == b[8];
        println!("rebuild from {} photo and {} version sidecars ({}): total {:.1} s\n  walk {:.0} ms | stat of every file {:.0} ms ({} files) | state files {:.0} ms | read+parse photos {:.1} s | read+parse versions {:.1} s | insert {:.1} s | indexes {:.1} s", photos.len(), versions.len(), if cold { "cold cache" } else { "warm cache" }, total_ms / 1000.0, walk_ms, stat_ms, n_stat, state_ms, photos_ms / 1000.0, versions_ms / 1000.0, insert_ms / 1000.0, index_ms / 1000.0);
        println!("  rebuilt catalogue: {} photos, {} versions, {} keyword links; same structure as the original: {}", b[0], b[4], b[5], same_structure);
        out.insert(if cold { "rebuild_cold" } else { "rebuild_warm" }.into(), json!({ "total_s": total_ms / 1000.0, "walk_ms": walk_ms, "stat_ms": stat_ms, "state_ms": state_ms, "photos_s": photos_ms / 1000.0, "versions_s": versions_ms / 1000.0, "insert_s": insert_ms / 1000.0, "indexes_s": index_ms / 1000.0, "same_structure": same_structure, "rows": b }));
    }
    if let Some(o) = get("--out") { std::fs::write(o, serde_json::to_string_pretty(&out)?)?; }
    Ok(())
}
