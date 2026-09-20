//! A realistic synthetic dataset: shoots of a few hundred photos, hierarchical keywords, bursts
//! that form series, several versions per photo, rating overrides on versions.

use fastrand::Rng;

pub struct KeywordNode {
    pub id: i64,
    pub parent: Option<i64>,
    pub name: String,
    pub path: String,
}

pub struct VersionRow {
    pub id: i64,
    pub photo_id: i64,
    pub name: String,
    pub rating: Option<i64>,
    pub flag: Option<i64>,
    pub label: Option<i64>,
    pub updated: i64,
    pub extra_keywords: Vec<i64>,
}

pub struct PhotoRow {
    pub id: i64,
    pub source_id: i64,
    pub path: String,
    pub filename: String,
    pub fingerprint: [u8; 16],
    pub capture_time: i64,
    pub camera_id: i64,
    pub lens_id: i64,
    pub iso: i64,
    pub aperture: f64,
    pub shutter: f64,
    pub focal: f64,
    pub width: i64,
    pub height: i64,
    pub rating: i64,
    pub flag: i64,
    pub label: i64,
    pub caption: Option<String>,
    pub gps: Option<(f64, f64)>,
    pub series_id: Option<i64>,
    pub stack_visible: bool,
    pub keywords: Vec<i64>,
    pub versions: Vec<VersionRow>,
    pub main_version_id: i64,
    pub effective_rating: i64,
}

pub struct Dataset {
    pub cameras: Vec<String>,
    pub lenses: Vec<String>,
    pub keywords: Vec<KeywordNode>,
    pub photos: Vec<PhotoRow>,
    pub series: Vec<(i64, i64)>, // (id, cover photo)
    pub collections: Vec<(i64, String, Vec<i64>)>,
}

const SYL: [&str; 24] = [
    "ba", "ko", "ri", "mu", "sel", "tan", "vo", "lin", "dar", "fe", "gro", "hy", "jo", "ka", "lu", "mor", "ne", "pa", "qui", "ros", "sti", "tur", "wen", "zy",
];

/// A pronounceable word for an index: two or three syllables, deterministic.
pub fn word(i: usize) -> String {
    let (a, b, c) = (i % 24, (i / 24) % 24, i / 576);
    if c == 0 { format!("{}{}", SYL[a], SYL[b]) } else { format!("{}{}{}", SYL[a], SYL[b], SYL[c % 24]) }
}

pub const VOCAB: usize = 1200;

pub fn generate(n_photos: usize, seed: u64) -> Dataset {
    let mut rng = Rng::with_seed(seed);
    let cameras: Vec<String> = ["Sony ILCE-7RM4", "Sony ILCE-7M3", "Nikon D850", "Nikon Z6", "Canon EOS R5", "Canon EOS 5D Mark IV", "Fujifilm X-T4", "Fujifilm X-T50", "Panasonic S5", "Olympus E-M5 III", "Leica Q2", "Pentax K-3 III"]
        .iter().map(|s| s.to_string()).collect();
    let lenses: Vec<String> = (0..30).map(|i| format!("Lens {}mm f/{}", [14, 16, 24, 28, 35, 50, 85, 105, 135, 200][i % 10], [1.4, 1.8, 2.8, 4.0][i % 4])).collect();

    // The keyword vocabulary: 20 categories, 20 groups each, 10 keywords each.
    let mut keywords = Vec::new();
    let mut leaves = Vec::new();
    let mut wi = 0usize;
    let next = |w: &mut usize| { *w += 1; word(*w % VOCAB) };
    for _ in 0..20 {
        let cat = next(&mut wi);
        let cid = keywords.len() as i64 + 1;
        keywords.push(KeywordNode { id: cid, parent: None, name: cat.clone(), path: cat.clone() });
        for _ in 0..20 {
            let grp = next(&mut wi);
            let gid = keywords.len() as i64 + 1;
            let gpath = format!("{cat}/{grp}");
            keywords.push(KeywordNode { id: gid, parent: Some(cid), name: grp, path: gpath.clone() });
            for _ in 0..10 {
                let leaf = next(&mut wi);
                let lid = keywords.len() as i64 + 1;
                keywords.push(KeywordNode { id: lid, parent: Some(gid), name: leaf.clone(), path: format!("{gpath}/{leaf}") });
                leaves.push(lid);
            }
        }
    }

    let shoots = (n_photos / 400).max(1);
    let t0 = 1_420_070_400i64; // 2015-01-01
    let span = 11 * 365 * 86_400i64;
    let iso_list = [100, 200, 400, 800, 1600, 3200, 6400, 12800];
    let ap_list = [1.4, 1.8, 2.8, 4.0, 5.6, 8.0, 11.0];
    let sh_list = [1.0 / 30.0, 1.0 / 60.0, 1.0 / 125.0, 1.0 / 250.0, 1.0 / 500.0, 1.0 / 1000.0, 1.0 / 2000.0];
    let focal_list = [14.0, 24.0, 35.0, 50.0, 85.0, 105.0, 135.0, 200.0];
    let mut photos: Vec<PhotoRow> = Vec::with_capacity(n_photos);
    let mut series: Vec<(i64, i64)> = Vec::new();
    let mut version_id = 0i64;

    for s in 0..shoots {
        let count = if s + 1 == shoots { n_photos - photos.len() } else { (n_photos / shoots).min(n_photos - photos.len()) };
        let start = t0 + span * s as i64 / shoots as i64 + rng.i64(0..86_400 * 5);
        let camera = rng.usize(0..cameras.len());
        let lens_pool: Vec<usize> = (0..3).map(|_| rng.usize(0..lenses.len())).collect();
        let pool: Vec<i64> = (0..14).map(|_| leaves[rng.usize(0..leaves.len())]).collect();
        let base_kw: Vec<i64> = pool[..4].to_vec();
        let loc = if rng.f32() < 0.4 { Some((rng.f64() * 100.0 - 20.0, rng.f64() * 200.0 - 100.0)) } else { None };
        let (w, h) = [(9504, 6336), (8256, 5504), (8192, 5464), (6000, 4000), (5184, 3888)][camera % 5];
        let ext = ["ARW", "NEF", "CR3", "RAF", "RW2", "ORF"][camera % 6];
        let mut t = start;
        let mut i = 0;
        while i < count {
            let burst = if rng.f32() < 0.03 { rng.usize(3..9) } else { 1 };
            let burst = burst.min(count - i);
            let series_id = if burst > 1 { Some(series.len() as i64 + 1) } else { None };
            for b in 0..burst {
                let id = photos.len() as i64 + 1;
                t += if burst > 1 { 1 } else { rng.i64(2..90) };
                let mut kw = base_kw.clone();
                for _ in 0..rng.usize(0..7) {
                    let k = pool[rng.usize(4..pool.len())];
                    if !kw.contains(&k) {
                        kw.push(k);
                    }
                }
                let r = rng.f32();
                let rating = if r < 0.55 { 0 } else if r < 0.60 { 1 } else if r < 0.68 { 2 } else if r < 0.80 { 3 } else if r < 0.92 { 4 } else { 5 };
                let f = rng.f32();
                let flag = if f < 0.75 { 0 } else if f < 0.90 { 1 } else { 2 };
                let caption = if rng.f32() < 0.3 { Some((0..rng.usize(4..11)).map(|_| word(rng.usize(0..VOCAB))).collect::<Vec<_>>().join(" ")) } else { None };
                let mut fp = [0u8; 16];
                for x in fp.iter_mut() {
                    *x = rng.u8(..);
                }
                // Versions: 70% one, 20% two, 7% three, 3% four.
                let v = rng.f32();
                let nv = if v < 0.70 { 1 } else if v < 0.90 { 2 } else if v < 0.97 { 3 } else { 4 };
                let mut versions = Vec::new();
                for k in 0..nv {
                    version_id += 1;
                    let over = k > 0 && rng.f32() < 0.35;
                    versions.push(VersionRow {
                        id: version_id,
                        photo_id: id,
                        name: if k == 0 { "Base".into() } else { format!("Version {}", k + 1) },
                        rating: if over { Some(rng.i64(0..6)) } else { None },
                        flag: None,
                        label: None,
                        updated: t + 3600 * (k as i64 + 1) + rng.i64(0..86_400),
                        extra_keywords: if k > 0 && rng.f32() < 0.1 { vec![leaves[rng.usize(0..leaves.len())]] } else { vec![] },
                    });
                }
                let main = versions.iter().max_by_key(|v| v.updated).unwrap();
                let effective_rating = main.rating.unwrap_or(rating);
                let main_version_id = main.id;
                photos.push(PhotoRow {
                    id,
                    source_id: 1 + (s as i64 % 3),
                    path: format!("/archive/{}/shoot-{:04}/DSC{:05}.{ext}", 2015 + (t - t0) / (365 * 86_400), s, id % 100_000),
                    filename: format!("DSC{:05}.{ext}", id % 100_000),
                    fingerprint: fp,
                    capture_time: t,
                    camera_id: camera as i64 + 1,
                    lens_id: lens_pool[rng.usize(0..3)] as i64 + 1,
                    iso: iso_list[rng.usize(0..iso_list.len())],
                    aperture: ap_list[rng.usize(0..ap_list.len())],
                    shutter: sh_list[rng.usize(0..sh_list.len())],
                    focal: focal_list[rng.usize(0..focal_list.len())],
                    width: w,
                    height: h,
                    rating,
                    flag,
                    label: if rng.f32() < 0.85 { 0 } else { rng.i64(1..9) },
                    caption,
                    gps: loc.map(|(la, lo)| (la + rng.f64() * 0.01, lo + rng.f64() * 0.01)),
                    series_id,
                    stack_visible: b == 0,
                    keywords: kw,
                    versions,
                    main_version_id,
                    effective_rating,
                });
            }
            if let Some(sid) = series_id {
                series.push((sid, photos.len() as i64 - burst as i64 + 1));
            }
            i += burst;
        }
    }

    // Collections: 200 manual ones of 20 to 2,000 photos.
    let mut collections = Vec::new();
    for c in 0..200 {
        let size = rng.usize(20..2000).min(n_photos);
        let first = rng.usize(0..n_photos - size + 1);
        let ids: Vec<i64> = (0..size).map(|k| photos[first + k].id).collect();
        collections.push((c as i64 + 1, format!("Collection {c}"), ids));
    }
    Dataset { cameras, lenses, keywords, photos, series, collections }
}
