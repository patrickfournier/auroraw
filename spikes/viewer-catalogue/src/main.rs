//! Spike 3, end to end: a Slint grid fed by the SQLite catalogue and the blob store of thumbnails,
//! with thumbnails loaded on worker threads. Scripted benchmark. Throwaway code.
//!
//! usage: viewer-catalogue [--secs N] [--cold] [--out result.json]

use catalogue::{data_dir, evict, pct};
use rusqlite::{Connection, OpenFlags};
use serde_json::json;
use slint::{ComponentHandle, Image, Model, ModelNotify, ModelRc, ModelTracker, RenderingState, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode};
use std::cell::{Cell as StdCell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

slint::include_modules!();

const COLS: usize = 8;
const ROW_H: f32 = 124.0;
const CACHE: usize = 800;

#[derive(Clone, Copy)]
struct Lite { id: i64, rating: i32 }

struct Shared {
    queue: Mutex<Vec<i64>>,
    cv: Condvar,
    inbox: Mutex<Vec<(i64, SharedPixelBuffer<Rgba8Pixel>)>>,
    stop: AtomicBool,
}

struct State {
    list: RefCell<Vec<Lite>>,
    index: RefCell<HashMap<i64, usize>>,
    cache: RefCell<HashMap<i64, Image>>,
    order: RefCell<VecDeque<i64>>,
    pending: RefCell<HashMap<i64, Instant>>,
    latencies: RefCell<Vec<f64>>,
    hits: StdCell<u64>,
    misses: StdCell<u64>,
    shared: Arc<Shared>,
    notify: ModelNotify,
}

struct RowModel(Rc<State>);

impl Model for RowModel {
    type Data = GridRow;
    fn row_count(&self) -> usize { self.0.list.borrow().len().div_ceil(COLS) }
    fn row_data(&self, row: usize) -> Option<GridRow> {
        let st = &self.0;
        let list = st.list.borrow();
        let cells: Vec<Cell> = (0..COLS).filter_map(|c| list.get(row * COLS + c)).map(|p| {
            if let Some(img) = st.cache.borrow().get(&p.id) {
                st.hits.set(st.hits.get() + 1);
                Cell { thumb: img.clone(), rating: p.rating, ready: true }
            } else {
                st.misses.set(st.misses.get() + 1);
                if !st.pending.borrow().contains_key(&p.id) {
                    st.pending.borrow_mut().insert(p.id, Instant::now());
                    let mut q = st.shared.queue.lock().unwrap();
                    q.push(p.id);
                    if q.len() > 400 { // fast scrolling: the oldest requests are no longer on screen
                        let excess = q.len() - 400;
                        let drop: Vec<i64> = q.drain(..excess).collect();
                        for id in drop { st.pending.borrow_mut().remove(&id); }
                    }
                    st.shared.cv.notify_one();
                }
                Cell { thumb: Image::default(), rating: p.rating, ready: false }
            }
        }).collect();
        Some(GridRow { cells: ModelRc::new(slint::VecModel::from(cells)) })
    }
    fn model_tracker(&self) -> &dyn ModelTracker { &self.0.notify }
}

fn query(db: &Connection, kind: i32) -> (Vec<Lite>, f64) {
    let t = Instant::now();
    let range = |q: &str| -> (String, String) { let p: String = db.query_row(q, [], |r| r.get(0)).unwrap(); (format!("{p}/"), format!("{p}0")) };
    let kw = "AND id IN (SELECT pk.photo_id FROM photo_keyword pk JOIN keyword k ON k.id=pk.keyword_id WHERE k.path >= ?1 AND k.path < ?2)";
    let (sql, lo, hi) = match kind {
        1 => ("SELECT id, effective_rating FROM photo WHERE stack_visible=1 AND effective_rating>=4 ORDER BY capture_time DESC".to_string(), String::new(), String::new()),
        2 => { let (a, b) = range("SELECT path FROM keyword WHERE parent_id IS NOT NULL AND path NOT LIKE '%/%/%' LIMIT 1 OFFSET 7"); (format!("SELECT id, effective_rating FROM photo WHERE stack_visible=1 {kw} ORDER BY capture_time DESC"), a, b) }
        3 => { let (a, b) = range("SELECT path FROM keyword WHERE parent_id IS NULL LIMIT 1 OFFSET 3"); (format!("SELECT id, effective_rating FROM photo WHERE stack_visible=1 {kw} ORDER BY capture_time DESC"), a, b) }
        _ => ("SELECT id, effective_rating FROM photo WHERE stack_visible=1 ORDER BY capture_time DESC".to_string(), String::new(), String::new()),
    };
    let mut st = db.prepare(&sql).unwrap();
    let map = |r: &rusqlite::Row| Ok(Lite { id: r.get(0)?, rating: r.get::<_, i64>(1)? as i32 });
    let list: Vec<Lite> = if lo.is_empty() { st.query_map([], map).unwrap().map(|r| r.unwrap()).collect() } else { st.query_map([&lo, &hi], map).unwrap().map(|r| r.unwrap()).collect() };
    (list, t.elapsed().as_secs_f64() * 1000.0)
}

struct App {
    ui: slint::Weak<GridWindow>,
    st: Rc<State>,
    db: Connection,
    secs: u64,
    frame: StdCell<usize>,
    stamps: RefCell<Vec<Instant>>,
    phase: StdCell<u8>,
    phase_start: StdCell<usize>,
    scroll: StdCell<f32>,
    empty_frames: StdCell<u64>,
    counted: StdCell<u64>,
    jump_t0: StdCell<Option<Instant>>,
    jump_times: RefCell<Vec<f64>>,
    jumps: StdCell<usize>,
    filter_results: RefCell<Vec<(i32, f64, f64, usize)>>,
    filter_idx: StdCell<usize>,
    filter_t0: StdCell<Option<(Instant, f64)>>,
    rng: RefCell<fastrand::Rng>,
    scroll_lat: RefCell<Vec<f64>>,
    scroll_range: StdCell<(usize, usize)>,
    out: Option<String>,
}

thread_local! { static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) }; }

impl App {
    fn apply_filter(&self, kind: i32) -> f64 {
        let (list, q_ms) = query(&self.db, kind);
        let st = &self.st;
        *st.index.borrow_mut() = list.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
        *st.list.borrow_mut() = list;
        st.pending.borrow_mut().clear();
        st.shared.queue.lock().unwrap().clear();
        st.notify.reset();
        q_ms
    }

    fn visible_range(&self) -> (usize, usize) {
        let first = (self.scroll.get() / ROW_H) as usize;
        (first, first + 7)
    }

    /// The number of visible cells that have no picture yet.
    fn empty_visible(&self) -> usize {
        let (a, b) = self.visible_range();
        let list = self.st.list.borrow();
        let cache = self.st.cache.borrow();
        (a * COLS..(b * COLS).min(list.len())).filter(|i| !cache.contains_key(&list[*i].id)).count()
    }

    fn set_scroll(&self, y: f32) {
        self.scroll.set(y);
        if let Some(ui) = self.ui.upgrade() { ui.set_scroll(-y); }
    }

    fn drain(&self) {
        let items: Vec<_> = std::mem::take(&mut *self.st.shared.inbox.lock().unwrap());
        if items.is_empty() { return; }
        let st = &self.st;
        let mut rows = std::collections::BTreeSet::new();
        for (id, buf) in items {
            if let Some(t0) = st.pending.borrow_mut().remove(&id) { st.latencies.borrow_mut().push(t0.elapsed().as_secs_f64() * 1000.0); }
            st.cache.borrow_mut().insert(id, Image::from_rgba8(buf));
            st.order.borrow_mut().push_back(id);
            if st.order.borrow().len() > CACHE { let old = st.order.borrow_mut().pop_front().unwrap(); st.cache.borrow_mut().remove(&old); }
            if let Some(i) = st.index.borrow().get(&id) { rows.insert(i / COLS); }
        }
        for r in rows { st.notify.row_changed(r); }
    }

    fn finish(&self) {
        let stamps = self.stamps.borrow();
        // Frames of the scrolling phase only.
        let (a, b) = (self.scroll_range.get().0, self.scroll_range.get().1);
        let iv: Vec<f64> = stamps.windows(2).enumerate().filter(|(i, _)| *i >= a && *i < b).map(|(_, w)| w[1].duration_since(w[0]).as_secs_f64() * 1000.0).collect();
        let lat = self.st.latencies.borrow();
        let (h, m) = (self.st.hits.get(), self.st.misses.get());
        let names = ["all", "rating 4+", "keyword group", "keyword category"];
        let filters: Vec<serde_json::Value> = self.filter_results.borrow().iter().map(|(k, q, full, n)| json!({ "filter": names[*k as usize], "query_ms": q, "to_full_page_ms": full, "photos": n })).collect();
        let result = json!({
            "list_len": self.st.list.borrow().len(),
            "scroll": { "frames": iv.len(), "interval_ms": { "median": pct(&iv, 0.5), "p95": pct(&iv, 0.95), "p99": pct(&iv, 0.99), "max": pct(&iv, 1.0) }, "frames_over_20ms": iv.iter().filter(|v| **v > 20.0).count(),
                "frames_with_an_empty_cell": self.empty_frames.get(), "frames_checked": self.counted.get() },
            "thumbnail_latency_ms": { "median": pct(&lat, 0.5), "p95": pct(&lat, 0.95), "p99": pct(&lat, 0.99), "max": pct(&lat, 1.0), "count": lat.len() },
            "cell_lookups": { "hits": h, "misses": m },
            "jumps_to_full_page_ms": { "median": pct(&self.jump_times.borrow(), 0.5), "p95": pct(&self.jump_times.borrow(), 0.95), "max": pct(&self.jump_times.borrow(), 1.0), "count": self.jump_times.borrow().len() },
            "filters": filters,
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        if let Some(o) = &self.out { let _ = std::fs::write(o, serde_json::to_string_pretty(&result).unwrap()); }
        self.st.shared.stop.store(true, Ordering::Relaxed);
        self.st.shared.cv.notify_all();
        if let Some(ui) = self.ui.upgrade() { let _ = ui.hide(); }
        let _ = slint::quit_event_loop();
    }

    /// Called once per presented frame.
    fn tick(&self) {
        let f = self.frame.get();
        self.frame.set(f + 1);
        let now_frames = f;
        let elapsed = now_frames - self.phase_start.get();
        match self.phase.get() {
            0 if elapsed >= 30 => { self.phase.set(1); self.phase_start.set(now_frames); self.scroll_range.set((now_frames, now_frames)); }
            1 => {
                // Scrolling 45 px per frame down the whole list, pictures loading as it goes.
                let total = self.st.list.borrow().len().div_ceil(COLS) as f32 * ROW_H;
                self.set_scroll((elapsed as f32 * 45.0) % (total - 800.0).max(1.0));
                self.counted.set(self.counted.get() + 1);
                if self.empty_visible() > 0 { self.empty_frames.set(self.empty_frames.get() + 1); }
                if elapsed as u64 > self.secs * 60 { self.scroll_range.set((self.scroll_range.get().0, now_frames)); self.phase.set(2); self.phase_start.set(now_frames); self.jumps.set(0); self.jump_t0.set(None); }
            }
            2 => {
                // Jumping to random places, as with the scrollbar: how long until the page is full?
                match self.jump_t0.get() {
                    None => {
                        let total = self.st.list.borrow().len().div_ceil(COLS) as f32 * ROW_H;
                        let y = self.rng.borrow_mut().f32() * (total - 800.0);
                        self.set_scroll(y);
                        self.jump_t0.set(Some(Instant::now()));
                        self.phase_start.set(now_frames);
                    }
                    Some(t0) => {
                        if self.empty_visible() == 0 || t0.elapsed() > Duration::from_secs(3) {
                            self.jump_times.borrow_mut().push(t0.elapsed().as_secs_f64() * 1000.0);
                            self.jump_t0.set(None);
                            self.jumps.set(self.jumps.get() + 1);
                            if self.jumps.get() >= 25 { self.phase.set(3); self.phase_start.set(now_frames); self.filter_idx.set(0); self.filter_t0.set(None); }
                        }
                    }
                }
            }
            3 => {
                let seq = [0, 1, 2, 3, 0, 1, 2, 3];
                match self.filter_t0.get() {
                    None => {
                        let i = self.filter_idx.get();
                        if i >= seq.len() { self.finish(); return; }
                        self.set_scroll(0.0);
                        let q = self.apply_filter(seq[i]);
                        self.filter_t0.set(Some((Instant::now(), q)));
                    }
                    Some((t0, q)) => {
                        if self.empty_visible() == 0 || t0.elapsed() > Duration::from_secs(3) {
                            let i = self.filter_idx.get();
                            self.filter_results.borrow_mut().push((seq[i], q, t0.elapsed().as_secs_f64() * 1000.0 + q, self.st.list.borrow().len()));
                            self.filter_idx.set(i + 1);
                            self.filter_t0.set(None);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let secs: u64 = get("--secs").and_then(|v| v.parse().ok()).unwrap_or(10);
    std::thread::spawn(move || { std::thread::sleep(Duration::from_secs(secs + 120)); eprintln!("watchdog: exiting"); std::process::exit(3); });
    let dir = data_dir();
    let (db_path, thumb_path) = (dir.join("catalogue.db"), dir.join("thumbs-32k.db"));
    if args.iter().any(|a| a == "--cold") { for p in [&db_path, &thumb_path] { evict(p); } }

    let shared = Arc::new(Shared { queue: Mutex::new(Vec::new()), cv: Condvar::new(), inbox: Mutex::new(Vec::new()), stop: AtomicBool::new(false) });
    // Four worker threads: read a blob, decode the JPEG, hand the pixels to the UI thread.
    for _ in 0..4 {
        let sh = shared.clone();
        let tp = thumb_path.clone();
        std::thread::spawn(move || {
            let db = Connection::open_with_flags(&tp, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
            let mut st = db.prepare("SELECT data FROM t WHERE id=?").unwrap();
            loop {
                let id = {
                    let mut q = sh.queue.lock().unwrap();
                    loop {
                        if sh.stop.load(Ordering::Relaxed) { return; }
                        if let Some(id) = q.pop() { break id; } // the most recent request first
                        q = sh.cv.wait(q).unwrap();
                    }
                };
                if let Ok(data) = st.query_row([id], |r| r.get::<_, Vec<u8>>(0)) {
                    if let Ok(img) = image::load_from_memory_with_format(&data, image::ImageFormat::Jpeg) {
                        let rgba = img.to_rgba8();
                        sh.inbox.lock().unwrap().push((id, SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba.as_raw(), rgba.width(), rgba.height())));
                    }
                }
            }
        });
    }

    let db = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let t = Instant::now();
    let (list, q_ms) = query(&db, 0);
    eprintln!("opened the catalogue and listed {} photos in {:.1} ms (query {:.1} ms)", list.len(), t.elapsed().as_secs_f64() * 1000.0, q_ms);
    let st = Rc::new(State {
        index: RefCell::new(list.iter().enumerate().map(|(i, p)| (p.id, i)).collect()), list: RefCell::new(list),
        cache: RefCell::new(HashMap::new()), order: RefCell::new(VecDeque::new()), pending: RefCell::new(HashMap::new()), latencies: RefCell::new(Vec::new()),
        hits: StdCell::new(0), misses: StdCell::new(0), shared, notify: ModelNotify::default(),
    });
    let ui = GridWindow::new().unwrap();
    ui.set_rows(ModelRc::new(RowModel(st.clone())));
    ui.set_stats(format!("{} photos", st.list.borrow().len()).into());
    let app = Rc::new(App {
        ui: ui.as_weak(), st: st.clone(), db, secs, frame: StdCell::new(0), stamps: RefCell::new(Vec::new()), phase: StdCell::new(0), phase_start: StdCell::new(0), scroll: StdCell::new(0.0),
        empty_frames: StdCell::new(0), counted: StdCell::new(0), jump_t0: StdCell::new(None), jump_times: RefCell::new(Vec::new()), jumps: StdCell::new(0), filter_results: RefCell::new(Vec::new()),
        filter_idx: StdCell::new(0), filter_t0: StdCell::new(None), rng: RefCell::new(fastrand::Rng::with_seed(5)), scroll_lat: RefCell::new(Vec::new()), scroll_range: StdCell::new((0, 0)), out: get("--out"),
    });
    APP.with(|a| *a.borrow_mut() = Some(app.clone()));
    let a2 = app.clone();
    ui.on_filter(move |k| { let q = a2.apply_filter(k); if let Some(u) = a2.ui.upgrade() { u.set_stats(format!("filter {k}: {} photos in {q:.1} ms", a2.st.list.borrow().len()).into()); } });

    // Pictures arrive on other threads; the UI thread picks them up every few milliseconds.
    let timer = Timer::default();
    { let a = app.clone(); timer.start(TimerMode::Repeated, Duration::from_millis(4), move || a.drain()); }
    ui.window().set_rendering_notifier(move |state, _| {
        if let RenderingState::AfterRendering = state {
            let _ = slint::invoke_from_event_loop(|| APP.with(|a| { if let Some(app) = a.borrow().as_ref() { app.stamps.borrow_mut().push(Instant::now()); app.tick(); if let Some(u) = app.ui.upgrade() { u.window().request_redraw(); } } }));
        }
    }).unwrap();
    ui.window().set_size(slint::PhysicalSize::new(1500, 900));
    ui.show().unwrap();
    ui.run().unwrap();
}
