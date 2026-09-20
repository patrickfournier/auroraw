//! Spike 2, iced: the same image view and grid as the Slint prototype, with scripted benchmarks.
//! iced has no virtualised list, so the grid is virtualised by hand. Throwaway code.
//!
//! usage: viewer-iced --bench view|grid|both [--view WxH] [--secs N] [--out result.json]

use gpu_pipeline::{gpu::{Gpu, Renderer}, raw, scene};
use iced::widget::{button, column, container, image, row, scrollable, space, text, text_input};
use iced::{ContentFit, Element, Length, Size, Subscription, Task, window};
use serde_json::json;
use std::path::Path;
use std::time::{Duration, Instant};

const ROW_H: f32 = 124.0;
const PER_ROW: usize = 8;
const ITEMS: usize = 100_000;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    View,
    Grid,
    Both,
}

#[derive(Debug, Clone)]
enum Message {
    Tick(Instant),
    Scrolled(scrollable::Viewport),
    Query(String),
    Language,
}

struct State {
    mode: Mode,
    frames: Vec<Vec<u8>>,
    vw: u32,
    vh: u32,
    frame: image::Handle,
    thumbs: Vec<image::Handle>,
    offset: f32,
    view_h: f32,
    query: String,
    french: bool,
    secs: u64,
    out: Option<String>,
    stamps: Vec<Instant>,
    update_ms: Vec<f64>,
    n: usize,
    started: Option<Instant>,
    boot: Instant,
    first_frame_ms: Option<f64>,
    prep_ms: f64,
}

fn rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
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

impl State {
    fn new() -> (Self, Task<Message>) {
        let boot = Instant::now();
        let args: Vec<String> = std::env::args().collect();
        let get = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
        let mode = match get("--bench").as_deref() {
            Some("grid") => Mode::Grid,
            Some("both") => Mode::Both,
            _ => Mode::View,
        };
        let (mut vw, mut vh) = if mode == Mode::Both { (1340u32, 480u32) } else { (1340, 964) };
        if let Some(v) = get("--view") {
            if let Some((a, b)) = v.split_once('x') {
                (vw, vh) = (a.parse().unwrap(), b.parse().unwrap());
            }
        }
        let t = Instant::now();
        let mut frames = Vec::new();
        if mode != Mode::Grid {
            let gpu: &'static Gpu = Box::leak(Box::new(Gpu::best().unwrap()));
            let sc = if Path::new("samples/Nikon-D850-14bit-compressed.NEF").exists() {
                raw::load(Path::new("samples/Nikon-D850-14bit-compressed.NEF"), false).unwrap()
            } else {
                scene::synthetic(6000, 4000)
            };
            let r = Renderer::new(gpu, &sc).unwrap();
            let vp = r.view_path(vw, vh);
            let (mx, my) = (sc.width - vw, sc.height - vh);
            for i in 0..60usize {
                let f = i as f64 / 59.0;
                let mut b = vec![0u8; (vw * vh * 4) as usize];
                r.render_view(&vp, (mx as f64 * f) as u32, (my as f64 * f) as u32, &mut b).unwrap();
                frames.push(b);
            }
        }
        let prep_ms = t.elapsed().as_secs_f64() * 1000.0;
        // 256 distinct small thumbnails.
        let thumbs: Vec<image::Handle> = (0..256usize)
            .map(|t| {
                let hue = t as f32 / 256.0 * 6.0;
                let (r, g, b) = ((hue.sin() * 0.5 + 0.5), ((hue + 2.0).sin() * 0.5 + 0.5), ((hue + 4.0).sin() * 0.5 + 0.5));
                let mut px = vec![255u8; 160 * 120 * 4];
                for y in 0..120usize {
                    for x in 0..160usize {
                        let k = 0.35 + 0.65 * ((x as f32 / 160.0) * (1.0 - y as f32 / 120.0)) + 0.08 * ((x as f32 * 0.4).sin() * (y as f32 * 0.3).cos());
                        let o = (y * 160 + x) * 4;
                        px[o] = (r * k * 255.0) as u8;
                        px[o + 1] = (g * k * 255.0) as u8;
                        px[o + 2] = (b * k * 255.0) as u8;
                    }
                }
                image::Handle::from_rgba(160, 120, px)
            })
            .collect();
        let frame = frames.first().map(|f| image::Handle::from_rgba(vw, vh, f.clone())).unwrap_or_else(|| thumbs[0].clone());
        (
            State {
                mode, frames, vw, vh, frame, thumbs, offset: 0.0, view_h: 900.0, query: String::new(), french: false,
                secs: get("--secs").and_then(|v| v.parse().ok()).unwrap_or(10), out: get("--out"),
                stamps: Vec::new(), update_ms: Vec::new(), n: 0, started: None, boot, first_frame_ms: None, prep_ms,
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick(now) => {
                if self.first_frame_ms.is_none() {
                    self.first_frame_ms = Some(self.boot.elapsed().as_secs_f64() * 1000.0 - self.prep_ms);
                }
                self.stamps.push(now);
                self.n += 1;
                if self.n == 30 {
                    self.started = Some(now);
                }
                if self.started.is_some_and(|s| now.duration_since(s) > Duration::from_secs(self.secs)) {
                    self.write_results();
                    return iced::exit();
                }
                let t = Instant::now();
                if matches!(self.mode, Mode::View | Mode::Both) && !self.frames.is_empty() {
                    self.frame = image::Handle::from_rgba(self.vw, self.vh, self.frames[self.n % self.frames.len()].clone());
                }
                let mut task = Task::none();
                if matches!(self.mode, Mode::Grid | Mode::Both) {
                    let total = (ITEMS / PER_ROW) as f32 * ROW_H;
                    self.offset = (self.n as f32 * 45.0) % (total - 600.0).max(1.0);
                    task = iced::widget::operation::scroll_to(GRID_ID.clone(), scrollable::AbsoluteOffset { x: 0.0, y: self.offset });
                }
                self.update_ms.push(t.elapsed().as_secs_f64() * 1000.0);
                task
            }
            Message::Scrolled(v) => {
                self.view_h = v.bounds().height;
                Task::none()
            }
            Message::Query(q) => {
                self.query = q;
                Task::none()
            }
            Message::Language => {
                self.french = !self.french;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let l = |en: &'static str, fr: &'static str| if self.french { fr } else { en };
        let sidebar = column![text(l("Keywords", "Mots-clés")).size(14)]
            .extend((0..20).map(|i| text(format!("▸ Category {i}")).size(14).into()))
            .spacing(4)
            .padding(8)
            .width(260);
        let topbar = row![
            text_input(l("Search", "Rechercher"), &self.query).on_input(Message::Query).width(320),
            button(text(l("Switch language", "Changer de langue"))).on_press(Message::Language),
        ]
        .spacing(8)
        .padding(4);
        let mut main = column![topbar];
        if matches!(self.mode, Mode::View | Mode::Both) {
            main = main.push(image(self.frame.clone()).width(self.vw as f32).height(self.vh as f32).content_fit(ContentFit::Fill).filter_method(image::FilterMethod::Nearest));
        }
        if matches!(self.mode, Mode::Grid | Mode::Both) {
            let total_rows = ITEMS.div_ceil(PER_ROW);
            let first = ((self.offset / ROW_H) as usize).saturating_sub(1);
            let last = (((self.offset + self.view_h.max(600.0)) / ROW_H) as usize + 2).min(total_rows - 1);
            let mut col = column![space().height(first as f32 * ROW_H)];
            for r in first..=last {
                let cells = (0..PER_ROW).map(|c| {
                    let id = r * PER_ROW + c;
                    let tile = (id * 2654435761) % 256;
                    image(self.thumbs[tile].clone()).width(160).height(120).into()
                });
                col = col.push(row(cells).spacing(4).height(ROW_H));
            }
            col = col.push(space().height((total_rows - 1 - last) as f32 * ROW_H));
            main = main.push(scrollable(col).id(GRID_ID.clone()).on_scroll(Message::Scrolled).width(Length::Fill).height(Length::Fill));
        }
        row![container(sidebar).style(container::rounded_box), main].into()
    }

    fn subscription(&self) -> Subscription<Message> {
        window::frames().map(Message::Tick)
    }

    fn write_results(&self) {
        let start = 30.min(self.stamps.len().saturating_sub(1));
        let iv: Vec<f64> = self.stamps.windows(2).skip(start).map(|w| w[1].duration_since(w[0]).as_secs_f64() * 1000.0).collect();
        let mean = iv.iter().sum::<f64>() / iv.len().max(1) as f64;
        let result = json!({
            "toolkit": "iced 0.14 (wgpu)", "mode": format!("{:?}", self.mode), "view": [self.vw, self.vh], "frames_measured": iv.len(),
            "frame_interval_ms": { "mean": mean, "median": pct(&iv, 0.5), "p95": pct(&iv, 0.95), "p99": pct(&iv, 0.99), "max": pct(&iv, 1.0) },
            "fps": 1000.0 / mean.max(1e-9), "frames_over_20ms": iv.iter().filter(|v| **v > 20.0).count(),
            "update_ms": { "median": pct(&self.update_ms, 0.5), "p95": pct(&self.update_ms, 0.95) },
            "first_frame_ms": self.first_frame_ms, "prepare_frames_ms": self.prep_ms, "rss_mb": rss_mb(),
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        if let Some(p) = &self.out {
            let _ = std::fs::write(p, serde_json::to_string_pretty(&result).unwrap());
        }
    }
}

static GRID_ID: std::sync::LazyLock<iced::widget::Id> = std::sync::LazyLock::new(|| iced::widget::Id::new("grid"));

fn main() -> iced::Result {
    // Safety net: never outlive the run by much.
    let a: Vec<String> = std::env::args().collect();
    let secs = a.iter().position(|x| x == "--secs").and_then(|i| a.get(i + 1)).and_then(|v| v.parse::<u64>().ok()).unwrap_or(10);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs + 40));
        eprintln!("watchdog: exiting");
        std::process::exit(3);
    });
    iced::application(State::new, State::update, State::view)
        .title("Auroraw spike 2 (iced)")
        .subscription(State::subscription)
        .window_size(Size::new(1600.0, 1000.0))
        .run()
}
