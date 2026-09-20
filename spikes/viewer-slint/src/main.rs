//! Spike 2, Slint: the image view, a virtualised grid of 100,000 thumbnails, a keyword tree, a
//! text field and a language switch, with scripted benchmarks. Throwaway code.
//!
//! usage: viewer-slint --bench view|grid|both|exact|tree [--view WxH] [--secs N]
//!        [--file raw] [--snapshot out.png] [--out result.json] [--dump-query]

use anyhow::Result;
use gpu_pipeline::{gpu::{Gpu, Renderer}, raw, scene};
use serde_json::json;
use slint::{ComponentHandle, Image, ModelRc, RenderingState, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::cell::{Cell as StdCell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

slint::include_modules!();

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    View,
    Grid,
    Both,
    Exact,
    Tree,
}

fn rss_mb() -> f64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    s.lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
        .unwrap_or(0.0)
}

fn pct(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() as f64 * q) as usize).min(s.len() - 1)]
}

/// 16 x 16 thumbnails of 160 x 120, drawn procedurally: enough variety for the grid.
fn make_atlas() -> (Vec<u8>, u32, u32) {
    let (tw, th, n) = (160usize, 120usize, 16usize);
    let (w, h) = (tw * n, th * n);
    let mut px = vec![255u8; w * h * 4];
    for ty in 0..n {
        for tx in 0..n {
            let id = ty * n + tx;
            let hue = id as f32 / 256.0 * 6.0;
            let (r, g, b) = ((hue.sin() * 0.5 + 0.5), ((hue + 2.0).sin() * 0.5 + 0.5), ((hue + 4.0).sin() * 0.5 + 0.5));
            for y in 0..th {
                for x in 0..tw {
                    let (fx, fy) = (x as f32 / tw as f32, y as f32 / th as f32);
                    let k = 0.35 + 0.65 * (fx * (1.0 - fy)) + 0.08 * ((x as f32 * 0.4).sin() * (y as f32 * 0.3).cos());
                    let o = ((ty * th + y) * w + tx * tw + x) * 4;
                    px[o] = (r * k * 255.0) as u8;
                    px[o + 1] = (g * k * 255.0) as u8;
                    px[o + 2] = (b * k * 255.0) as u8;
                }
            }
        }
    }
    (px, w as u32, h as u32)
}

fn make_rows(items: usize, per_row: usize, thumbs: &[Image]) -> Vec<GridRow> {
    (0..items.div_ceil(per_row))
        .map(|r| {
            let cells: Vec<Cell> = (0..per_row)
                .filter(|c| r * per_row + c < items)
                .map(|c| {
                    let id = r * per_row + c;
                    let tile = (id * 2654435761) % 256;
                    Cell { ax: ((tile % 16) * 160) as i32, ay: ((tile / 16) * 120) as i32, rating: (id % 6) as i32, thumb: thumbs[tile].clone() }
                })
                .collect();
            GridRow { cells: ModelRc::new(VecModel::from(cells)) }
        })
        .collect()
}

/// A keyword tree of about 4,400 nodes, shown as a flat list with indentation.
struct Tree {
    children: Vec<Vec<usize>>,
    labels: Vec<String>,
    depth: Vec<i32>,
    expanded: Vec<bool>,
}

impl Tree {
    fn new() -> Self {
        let mut t = Tree { children: vec![vec![]], labels: vec!["root".into()], depth: vec![-1], expanded: vec![true] };
        let add = |t: &mut Tree, parent: usize, label: String| {
            let id = t.children.len();
            t.children.push(vec![]);
            t.labels.push(label);
            t.depth.push(t.depth[parent] + 1);
            t.expanded.push(false);
            t.children[parent].push(id);
            id
        };
        for a in 0..20 {
            let na = add(&mut t, 0, format!("Category {a}"));
            for b in 0..20 {
                let nb = add(&mut t, na, format!("Group {a}.{b}"));
                for c in 0..10 {
                    add(&mut t, nb, format!("Keyword {a}.{b}.{c}"));
                }
            }
        }
        t
    }

    fn flatten(&self) -> (Vec<TreeRow>, Vec<usize>) {
        let (mut rows, mut ids) = (Vec::new(), Vec::new());
        let mut stack: Vec<usize> = self.children[0].iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            rows.push(TreeRow { depth: self.depth[n], label: self.labels[n].clone().into(), expanded: self.expanded[n], has_children: !self.children[n].is_empty() });
            ids.push(n);
            if self.expanded[n] {
                stack.extend(self.children[n].iter().rev());
            }
        }
        (rows, ids)
    }
}

#[derive(Default)]
struct Metrics {
    stamps: Vec<Instant>,
    render_ms: Vec<f64>,
    update_ms: Vec<f64>,
    snapshot_diff: Option<(usize, u32, usize)>,
}

fn main() -> Result<()> {
    let main_start = Instant::now();
    // Safety net: whatever happens, this process must not outlive its run by much.
    let watchdog_secs = {
        let a: Vec<String> = std::env::args().collect();
        let secs = a.iter().position(|x| x == "--secs").and_then(|i| a.get(i + 1)).and_then(|v| v.parse::<u64>().ok()).unwrap_or(10);
        secs + 40
    };
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(watchdog_secs));
        eprintln!("watchdog: still running after {watchdog_secs} s, exiting");
        std::process::exit(3);
    });
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let mode = match get("--bench").as_deref() {
        Some("grid") => Mode::Grid,
        Some("both") => Mode::Both,
        Some("exact") => Mode::Exact,
        Some("tree") => Mode::Tree,
        _ => Mode::View,
    };
    let secs: u64 = get("--secs").and_then(|v| v.parse().ok()).unwrap_or(10);
    let (mut vw, mut vh) = match mode {
        Mode::Both => (1340u32, 480u32),
        Mode::Exact => (800, 600),
        _ => (1340, 964),
    };
    if let Some(v) = get("--view") {
        if let Some((a, b)) = v.split_once('x') {
            (vw, vh) = (a.parse()?, b.parse()?);
        }
    }
    let show_view = matches!(mode, Mode::View | Mode::Both | Mode::Exact);
    let show_grid = matches!(mode, Mode::Grid | Mode::Both | Mode::Tree);

    // Frames: a pan across a real RAW file, developed by the spike 1 pipeline, held in memory so
    // that the measures below are those of the toolkit alone.
    let t_prep = Instant::now();
    let n_frames = 60usize;
    let mut frames: Vec<Vec<u8>> = Vec::new();
    if show_view {
        let gpu: &'static Gpu = Box::leak(Box::new(Gpu::select("vulkan nvidia").or_else(|_| Gpu::select("vulkan"))?));
        let scene = match get("--file") {
            Some(f) if Path::new(&f).exists() => raw::load(Path::new(&f), false)?,
            _ if Path::new("samples/Nikon-D850-14bit-compressed.NEF").exists() => raw::load(Path::new("samples/Nikon-D850-14bit-compressed.NEF"), false)?,
            _ => scene::synthetic(6000, 4000),
        };
        let r = Renderer::new(gpu, &scene)?;
        let vp = r.view_path(vw, vh);
        let (max_x, max_y) = (scene.width - vw, scene.height - vh);
        for i in 0..n_frames {
            let t = i as f64 / (n_frames - 1) as f64;
            let mut buf = vec![0u8; (vw * vh * 4) as usize];
            r.render_view(&vp, (max_x as f64 * t) as u32, (max_y as f64 * t) as u32, &mut buf)?;
            frames.push(buf);
        }
        eprintln!("{} frames of {vw}x{vh} prepared from {} in {:.0} ms", n_frames, scene.label, t_prep.elapsed().as_secs_f64() * 1000.0);
    }
    let prep_ms = t_prep.elapsed().as_secs_f64() * 1000.0;
    let frames = Arc::new(frames);

    let t_window = Instant::now();
    let ui = MainWindow::new()?;
    let (win_w, win_h) = if matches!(mode, Mode::Grid | Mode::Both | Mode::Tree) { (1600, 1000) } else { (260 + vw as i32, 36 + vh as i32) };
    ui.set_win_w(win_w);
    ui.set_win_h(win_h);
    ui.set_view_w(vw as i32);
    ui.set_view_h(vh as i32);
    ui.set_show_view(show_view);
    ui.set_show_grid(show_grid);
    ui.set_cell_images(!args.iter().any(|a| a == "--no-images"));
    ui.set_cell_text(!args.iter().any(|a| a == "--no-text"));
    if let Some(f) = frames.first() {
        let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(vw, vh);
        b.make_mut_bytes().copy_from_slice(f);
        ui.set_frame(Image::from_rgba8(b));
    }
    let (atlas, aw, ah) = make_atlas();
    ui.set_atlas(Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&atlas, aw, ah)));
    let t_rows = Instant::now();
    let items: usize = get("--items").and_then(|v| v.parse().ok()).unwrap_or(100_000);
    // 256 distinct small images of 160x120, shared by the cells (a real catalogue would have one per photo).
    let thumbs: Vec<Image> = (0..256usize)
        .map(|t| {
            let (tx, ty) = (t % 16, t / 16);
            let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(160, 120);
            let bytes = b.make_mut_bytes();
            for y in 0..120usize {
                let src = ((ty * 120 + y) * aw as usize + tx * 160) * 4;
                bytes[y * 160 * 4..(y + 1) * 160 * 4].copy_from_slice(&atlas[src..src + 160 * 4]);
            }
            Image::from_rgba8(b)
        })
        .collect();
    ui.set_use_atlas(args.iter().any(|a| a == "--atlas"));
    let rows = make_rows(items, 8, &thumbs);
    let row_count = rows.len();
    ui.set_rows(ModelRc::new(VecModel::from(rows)));
    eprintln!("grid model of {items} items built in {:.0} ms", t_rows.elapsed().as_secs_f64() * 1000.0);

    // The tree.
    let tree = Rc::new(RefCell::new(Tree::new()));
    let tree_ids = Rc::new(RefCell::new(Vec::<usize>::new()));
    let tree_model = Rc::new(VecModel::<TreeRow>::default());
    {
        let (rows, ids) = tree.borrow().flatten();
        tree_model.set_vec(rows);
        *tree_ids.borrow_mut() = ids;
    }
    ui.set_tree(ModelRc::from(tree_model.clone()));
    let toggle_ms = Rc::new(RefCell::new(Vec::<f64>::new()));
    {
        let (tree, tree_ids, tree_model, toggle_ms) = (tree.clone(), tree_ids.clone(), tree_model.clone(), toggle_ms.clone());
        ui.on_toggle_node(move |i| {
            let t = Instant::now();
            let node = tree_ids.borrow()[i as usize];
            {
                let mut tr = tree.borrow_mut();
                let e = tr.expanded[node];
                tr.expanded[node] = !e;
            }
            let (rows, ids) = tree.borrow().flatten();
            tree_model.set_vec(rows);
            *tree_ids.borrow_mut() = ids;
            toggle_ms.borrow_mut().push(t.elapsed().as_secs_f64() * 1000.0);
        });
    }

    // Language switch.
    let lang = Rc::new(StdCell::new(false));
    {
        let (weak, lang) = (ui.as_weak(), lang.clone());
        ui.on_toggle_language(move || {
            lang.set(!lang.get());
            let _ = slint::select_bundled_translation(if lang.get() { "fr" } else { "en" });
            let _ = weak.upgrade();
        });
    }
    let caption_en = ui.invoke_caption().to_string();
    let r_fr = slint::select_bundled_translation("fr");
    let caption_fr = ui.invoke_caption().to_string();
    let r_en = slint::select_bundled_translation("en");
    eprintln!("select fr: {r_fr:?}, select en: {r_en:?}");
    let _ = (&caption_en, &caption_fr);

    // The frame loop: each presented frame triggers the next update.
    let metrics = Arc::new(Mutex::new(Metrics::default()));
    let run_for = Duration::from_secs(secs);
    let counter = Rc::new(StdCell::new(0usize));
    let started = Rc::new(RefCell::new(None::<Instant>));
    let snapshot_path = get("--snapshot");
    let first_frame = Arc::new(Mutex::new(None::<f64>));
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let weak = ui.as_weak();
        let (metrics, frames, counter, started, first_frame, done) = (metrics.clone(), frames.clone(), counter.clone(), started.clone(), first_frame.clone(), done.clone());
        let tree_toggle = Rc::new(StdCell::new(0usize));
        let before = Rc::new(StdCell::new(None::<Instant>));
        ui.window().set_rendering_notifier(move |state, _| {
            if matches!(state, RenderingState::BeforeRendering) {
                before.set(Some(Instant::now()));
                return;
            }
            if !matches!(state, RenderingState::AfterRendering) {
                return;
            }
            if done.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            let now = Instant::now();
            if let Some(b) = before.take() {
                metrics.lock().unwrap().render_ms.push(now.duration_since(b).as_secs_f64() * 1000.0);
            }
            let n = counter.get();
            counter.set(n + 1);
            if n == 0 {
                *first_frame.lock().unwrap() = Some(t_window.elapsed().as_secs_f64() * 1000.0);
            }
            if n == 30 {
                *started.borrow_mut() = Some(now);
            }
            metrics.lock().unwrap().stamps.push(now);
            let finished = mode == Mode::Exact && n >= 45 || started.borrow().is_some_and(|s| now.duration_since(s) > run_for);
            if finished {
                done.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            let (metrics, frames, snap) = (metrics.clone(), frames.clone(), snapshot_path.clone());
            let tt = tree_toggle.get();
            tree_toggle.set(tt + 1);
            let _ = weak.upgrade_in_event_loop(move |ui| {
                let t = Instant::now();
                match mode {
                    Mode::View | Mode::Both if !frames.is_empty() => {
                        let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(vw, vh);
                        b.make_mut_bytes().copy_from_slice(&frames[n % frames.len()]);
                        ui.set_frame(Image::from_rgba8(b));
                    }
                    _ => {}
                }
                if matches!(mode, Mode::Grid | Mode::Both) {
                    let total = row_count as f32 * 124.0;
                    let span = (total - 600.0).max(1.0);
                    ui.set_grid_scroll(-((n as f32 * 45.0) % span));
                }
                if mode == Mode::Tree && n % 6 == 0 {
                    ui.invoke_toggle_node((tt % 20) as i32);
                }
                metrics.lock().unwrap().update_ms.push(t.elapsed().as_secs_f64() * 1000.0);
                if finished {
                    if let Ok(shot) = ui.window().take_snapshot() {
                        if mode == Mode::Exact {
                            // Compare the view region of the snapshot with the frame that was sent.
                            let (sw, sh) = (shot.width() as usize, shot.height() as usize);
                            let px = shot.as_bytes();
                            let rows = (vh as usize).min(sh.saturating_sub(36));
                            let cols = (vw as usize).min(sw.saturating_sub(260));
                            eprintln!("snapshot {sw}x{sh}, comparing {cols}x{rows} of the {vw}x{vh} view");
                            let (mut bad, mut worst) = (0usize, 0u32);
                            for y in 0..rows {
                                for x in 0..cols {
                                    let s = ((y + 36) * sw + x + 260) * 4;
                                    let f = (y * vw as usize + x) * 4;
                                    let d = (0..3).map(|c| (px[s + c] as i32 - frames[0][f + c] as i32).unsigned_abs()).max().unwrap();
                                    if d > 0 {
                                        bad += 1;
                                        worst = worst.max(d);
                                    }
                                }
                            }
                            metrics.lock().unwrap().snapshot_diff = Some((bad, worst, rows * cols));
                        }
                        if let Some(path) = snap {
                            let _ = image::save_buffer(path, shot.as_bytes(), shot.width(), shot.height(), image::ExtendedColorType::Rgba8);
                        }
                    }
                    let _ = ui.hide();
                    let _ = slint::quit_event_loop();
                } else {
                    ui.window().request_redraw();
                }
            });
        })?;
    }

    ui.window().set_size(slint::PhysicalSize::new(win_w as u32, win_h as u32));
    ui.show()?;
    ui.run()?;

    // The language switch, checked once the window has been through the event loop.
    slint::select_bundled_translation("en").ok();
    let after_en = ui.invoke_caption().to_string();
    slint::select_bundled_translation("fr").ok();
    let after_fr = ui.invoke_caption().to_string();
    slint::select_bundled_translation("en").ok();
    let i18n_ok = after_en == "Keywords" && after_fr == "Mots-clés";
    eprintln!("language switch after the event loop: '{after_en}' -> '{after_fr}' ({})", if i18n_ok { "ok" } else { "FAILED" });

    // Results.
    let m = metrics.lock().unwrap();
    let start = 30.min(m.stamps.len().saturating_sub(1));
    let intervals: Vec<f64> = m.stamps.windows(2).skip(start).map(|w| w[1].duration_since(w[0]).as_secs_f64() * 1000.0).collect();
    let mean = intervals.iter().sum::<f64>() / intervals.len().max(1) as f64;
    let over_20 = intervals.iter().filter(|v| **v > 20.0).count();
    let result = json!({
        "toolkit": "slint 1.18 (femtovg, winit)", "mode": format!("{mode:?}"),
        "view": [vw, vh], "window": [win_w, win_h], "frames_measured": intervals.len(),
        "frame_interval_ms": { "mean": mean, "median": pct(&intervals, 0.5), "p95": pct(&intervals, 0.95), "p99": pct(&intervals, 0.99), "max": pct(&intervals, 1.0) },
        "fps": 1000.0 / mean.max(1e-9), "frames_over_20ms": over_20,
        "update_ms": { "median": pct(&m.update_ms, 0.5), "p95": pct(&m.update_ms, 0.95) },
        "render_ms": { "median": pct(&m.render_ms[30.min(m.render_ms.len().saturating_sub(1))..], 0.5), "p95": pct(&m.render_ms[30.min(m.render_ms.len().saturating_sub(1))..], 0.95), "p99": pct(&m.render_ms[30.min(m.render_ms.len().saturating_sub(1))..], 0.99), "max": pct(&m.render_ms[30.min(m.render_ms.len().saturating_sub(1))..], 1.0) },
        "first_frame_ms": *first_frame.lock().unwrap(), "prepare_frames_ms": prep_ms,
        "rss_mb": rss_mb(), "i18n_ok": i18n_ok,
        "tree_toggle_ms_median": pct(&toggle_ms.borrow(), 0.5), "tree_toggles": toggle_ms.borrow().len(),
        "pixel_exact": m.snapshot_diff.map(|(bad, worst, total)| json!({ "different_pixels": bad, "worst_channel_difference": worst, "total_pixels": total })),
    });
    println!("{}", serde_json::to_string_pretty(&result)?);
    if let Some(path) = get("--out") {
        std::fs::write(path, serde_json::to_string_pretty(&result)?)?;
    }
    if args.iter().any(|a| a == "--dump-query") {
        println!("query: {:?}", ui.get_query());
    }
    Ok(())
}
