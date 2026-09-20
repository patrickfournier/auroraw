//! The wgpu side: adapter selection, the three compute passes, and the view paths measured by
//! the spike (full export by bands, 100% crop, fit-to-screen).

use crate::{Params, scene::Scene};
use anyhow::{Context, Result, anyhow};
use std::borrow::Cow;
use std::time::{Duration, Instant};

const COMMON: &str = include_str!("shaders/common.wgsl");

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
    pub limits: wgpu::Limits,
}

impl Gpu {
    /// Picks the first adapter whose "backend name" contains `selector` (case-insensitive).
    pub fn select(selector: &str) -> Result<Gpu> {
        let instance = wgpu::Instance::default();
        let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
        // Every word must appear in "<backend> <name>", so "vulkan nvidia" or "dx12" both work.
        let words: Vec<String> = selector.to_lowercase().split_whitespace().map(String::from).collect();
        let adapter = adapters
            .into_iter()
            .find(|a| {
                let i = a.get_info();
                let text = format!("{:?} {}", i.backend, i.name).to_lowercase();
                words.iter().all(|w| text.contains(w))
            })
            .with_context(|| format!("no adapter matches {selector:?}"))?;
        let info = adapter.get_info();
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: limits.clone(),
            ..Default::default()
        }))?;
        Ok(Gpu { device, queue, info, limits })
    }

    pub fn wait(&self) -> Result<()> {
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| anyhow!("poll: {e:?}"))?;
        Ok(())
    }
}

fn pipeline(device: &wgpu::Device, name: &str, body: &str) -> wgpu::ComputePipeline {
    let source = format!("{COMMON}\n{body}");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(name),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(name),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct Timing {
    pub compute_ms: f64,
    pub readback_ms: f64,
}

pub struct Renderer<'g> {
    g: &'g Gpu,
    demosaic: wgpu::ComputePipeline,
    tone: wgpu::ComputePipeline,
    downscale: wgpu::ComputePipeline,
    demosaic_cfa: wgpu::ComputePipeline,
    nlm: wgpu::ComputePipeline,
    blur: wgpu::ComputePipeline,
    combine: wgpu::ComputePipeline,
    /// Time to send the mosaic and the LUT to the GPU, in milliseconds.
    pub upload_ms: f64,
    cfa: Option<wgpu::Buffer>,
    mosaic: wgpu::Buffer,
    lut: wgpu::Buffer,
    pub scene_params: Params,
}

impl<'g> Renderer<'g> {
    pub fn new(g: &'g Gpu, scene: &Scene) -> Result<Self> {
        let d = &g.device;
        let demosaic = pipeline(d, "demosaic", include_str!("shaders/demosaic.wgsl"));
        let tone = pipeline(d, "tone", include_str!("shaders/tone.wgsl"));
        let downscale = pipeline(d, "downscale", include_str!("shaders/downscale.wgsl"));
        let demosaic_cfa = pipeline(d, "demosaic_cfa", include_str!("shaders/demosaic_cfa.wgsl"));
        let nlm = pipeline(d, "nlm", include_str!("shaders/nlm.wgsl"));
        let blur = pipeline(d, "blur", include_str!("shaders/blur.wgsl"));
        let combine = pipeline(d, "combine", include_str!("shaders/combine.wgsl"));
        let t_upload = Instant::now();
        let words: Vec<u32> = scene
            .mosaic
            .chunks(2)
            .map(|c| c[0] as u32 | ((*c.get(1).unwrap_or(&0) as u32) << 16))
            .collect();
        let mosaic = Self::make(g, (words.len() * 4) as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
        g.queue.write_buffer(&mosaic, 0, bytemuck::cast_slice(&words));
        let lut = Self::make(g, (scene.lut.len() * 16) as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
        g.queue.write_buffer(&lut, 0, bytemuck::cast_slice(&scene.lut));
        let cfa = scene.cfa6.as_ref().map(|t| {
            let b = Self::make(g, (t.len() * 4) as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
            g.queue.write_buffer(&b, 0, bytemuck::cast_slice(t));
            b
        });
        g.queue.submit([]);
        g.wait()?;
        let upload_ms = ms(t_upload.elapsed());
        Ok(Self { g, demosaic, tone, downscale, demosaic_cfa, nlm, blur, combine, upload_ms, cfa, mosaic, lut, scene_params: scene.params })
    }

    fn make(g: &Gpu, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
        g.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage, mapped_at_creation: false })
    }

    pub fn storage(&self, size: u64) -> wgpu::Buffer {
        Self::make(self.g, size, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST)
    }

    /// Records one compute pass with its own uniform block.
    fn pass(&self, enc: &mut wgpu::CommandEncoder, pl: &wgpu::ComputePipeline, p: &Params, buffers: &[&wgpu::Buffer]) {
        let uniform = Self::make(self.g, std::mem::size_of::<Params>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
        self.g.queue.write_buffer(&uniform, 0, bytemuck::bytes_of(p));
        let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }];
        for (i, b) in buffers.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: i as u32 + 1, resource: b.as_entire_binding() });
        }
        let bg = self.g.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pl.get_bind_group_layout(0),
            entries: &entries,
        });
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(pl);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(p.tile_w.div_ceil(16), p.tile_h.div_ceil(16), 1);
    }

    /// The demosaicing pass: the Bayer shader, or the generic one when the scene has a CFA table.
    fn demosaic_pass(&self, enc: &mut wgpu::CommandEncoder, p: &Params, inter: &wgpu::Buffer) {
        match &self.cfa {
            Some(table) => self.pass(enc, &self.demosaic_cfa, p, &[&self.mosaic, inter, table]),
            None => self.pass(enc, &self.demosaic, p, &[&self.mosaic, inter]),
        }
    }

    fn submit_wait(&self, enc: wgpu::CommandEncoder) -> Result<()> {
        self.g.queue.submit([enc.finish()]);
        self.g.wait()
    }

    fn encoder(&self) -> wgpu::CommandEncoder {
        self.g.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default())
    }

    /// Rows per band so that one band of camera RGB stays under the binding limit.
    pub fn band_rows(&self, width: u32, multiple_of: u32) -> u32 {
        let budget = (self.g.limits.max_storage_buffer_binding_size as u64).min(160 << 20);
        let rows = (budget / (width as u64 * 16)).clamp(16, 2048) as u32;
        (rows / multiple_of.max(1)).max(1) * multiple_of.max(1)
    }

    /// Full-resolution render of the whole image, by full-width bands, read back to the CPU.
    pub fn export(&self, rows_per_band: u32) -> Result<(Vec<u32>, Timing)> {
        let p0 = self.scene_params;
        let (w, h) = (p0.width, p0.height);
        let inter = self.storage(w as u64 * rows_per_band as u64 * 16);
        let out = self.storage(w as u64 * rows_per_band as u64 * 4);
        let readback = Self::make(self.g, w as u64 * h as u64 * 4, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
        let start = Instant::now();
        let mut y0 = 0;
        while y0 < h {
            let th = rows_per_band.min(h - y0);
            let p = Params { tile_x: 0, tile_y: y0, tile_w: w, tile_h: th, in_x0: 0, in_y0: 0, in_stride: w, out_x0: 0, out_y0: 0, out_stride: w, ..p0 };
            let mut enc = self.encoder();
            self.demosaic_pass(&mut enc, &p, &inter);
            self.pass(&mut enc, &self.tone, &p, &[&inter, &self.lut, &out]);
            enc.copy_buffer_to_buffer(&out, 0, &readback, y0 as u64 * w as u64 * 4, Some(th as u64 * w as u64 * 4));
            self.g.queue.submit([enc.finish()]);
            y0 += th;
        }
        self.g.wait()?;
        let compute = start.elapsed();
        let t1 = Instant::now();
        let pixels = read_words(self.g, &readback)?;
        Ok((pixels, Timing { compute_ms: ms(compute), readback_ms: ms(t1.elapsed()) }))
    }

    /// 100% view: develops a viewport-sized region from the mosaic. Returns the camera RGB cache.
    pub fn crop_recompute(&self, x0: u32, y0: u32, w: u32, h: u32, inter: &wgpu::Buffer, out: &wgpu::Buffer) -> Result<f64> {
        let p0 = self.scene_params;
        let p = Params { tile_x: x0, tile_y: y0, tile_w: w, tile_h: h, in_x0: 0, in_y0: 0, in_stride: w, out_x0: 0, out_y0: 0, out_stride: w, ..p0 };
        let start = Instant::now();
        let mut enc = self.encoder();
        self.demosaic_pass(&mut enc, &p, inter);
        self.pass(&mut enc, &self.tone, &p, &[inter, &self.lut, out]);
        self.submit_wait(enc)?;
        Ok(ms(start.elapsed()))
    }

    /// 100% view, downstream change only (exposure, tone, display): reruns the tone pass on the cache.
    pub fn crop_tone_only(&self, w: u32, h: u32, exposure: f32, inter: &wgpu::Buffer, out: &wgpu::Buffer) -> Result<f64> {
        let p0 = self.scene_params;
        let p = Params { tile_w: w, tile_h: h, in_x0: 0, in_y0: 0, in_stride: w, out_x0: 0, out_y0: 0, out_stride: w, exposure, ..p0 };
        let start = Instant::now();
        let mut enc = self.encoder();
        self.pass(&mut enc, &self.tone, &p, &[inter, &self.lut, out]);
        self.submit_wait(enc)?;
        Ok(ms(start.elapsed()))
    }

    /// Develops the whole image by bands and box-downscales the camera RGB to about `target_w`
    /// pixels wide. Returns (factor, width, height, buffer, milliseconds).
    pub fn fit_prepare(&self, target_w: u32) -> Result<(u32, u32, u32, wgpu::Buffer, f64)> {
        let p0 = self.scene_params;
        let (w, h) = (p0.width, p0.height);
        let f = w.div_ceil(target_w).max(1);
        let (dw, dh) = (w.div_ceil(f), h.div_ceil(f));
        let rows = self.band_rows(w, f);
        let inter = self.storage(w as u64 * rows as u64 * 16);
        let small_buf = self.storage(dw as u64 * dh as u64 * 16);
        let start = Instant::now();
        let mut y0 = 0;
        while y0 < h {
            let th = rows.min(h - y0);
            let demosaic = Params { tile_x: 0, tile_y: y0, tile_w: w, tile_h: th, ..p0 };
            let down = Params { tile_w: dw, tile_h: th.div_ceil(f), src_w: w, src_h: th, factor: f, out_y0: y0 / f, ..p0 };
            let mut enc = self.encoder();
            self.demosaic_pass(&mut enc, &demosaic, &inter);
            self.pass(&mut enc, &self.downscale, &down, &[&inter, &small_buf]);
            self.g.queue.submit([enc.finish()]);
            y0 += th;
        }
        self.g.wait()?;
        Ok((f, dw, dh, small_buf, ms(start.elapsed())))
    }

    /// Fit-to-screen, cold: `fit_prepare`, then the tone pass. Returns (factor, width, height, ms).
    pub fn fit_cold(&self, target_w: u32, small: &mut Option<wgpu::Buffer>, out: &mut Option<wgpu::Buffer>) -> Result<(u32, u32, u32, f64)> {
        let (f, dw, dh, small_buf, prep) = self.fit_prepare(target_w)?;
        let out_buf = self.storage(dw as u64 * dh as u64 * 4);
        let p0 = self.scene_params;
        let tone = Params { tile_w: dw, tile_h: dh, in_x0: 0, in_y0: 0, in_stride: dw, out_x0: 0, out_y0: 0, out_stride: dw, ..p0 };
        let start = Instant::now();
        let mut enc = self.encoder();
        self.pass(&mut enc, &self.tone, &tone, &[&small_buf, &self.lut, &out_buf]);
        self.submit_wait(enc)?;
        *small = Some(small_buf);
        *out = Some(out_buf);
        Ok((f, dw, dh, prep + ms(start.elapsed())))
    }

    /// Fit-to-screen, downstream change only: reruns the tone pass on the small cache.
    pub fn fit_tone_only(&self, dw: u32, dh: u32, exposure: f32, small: &wgpu::Buffer, out: &wgpu::Buffer) -> Result<f64> {
        self.crop_tone_only(dw, dh, exposure, small, out)
    }

    pub fn read(&self, buf: &wgpu::Buffer, words: u64) -> Result<Vec<u32>> {
        let readback = Self::make(self.g, words * 4, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
        let mut enc = self.encoder();
        enc.copy_buffer_to_buffer(buf, 0, &readback, 0, Some(words * 4));
        self.submit_wait(enc)?;
        read_words(self.g, &readback)
    }
}

/// Buffers for the 100% view path: develop a viewport, read it back to the CPU as RGBA8.
pub struct ViewPath {
    inter: wgpu::Buffer,
    out: wgpu::Buffer,
    readback: wgpu::Buffer,
    pub w: u32,
    pub h: u32,
}

impl<'g> Renderer<'g> {
    pub fn view_path(&self, w: u32, h: u32) -> ViewPath {
        ViewPath {
            inter: self.storage(w as u64 * h as u64 * 16),
            out: self.storage(w as u64 * h as u64 * 4),
            readback: Self::make(self.g, w as u64 * h as u64 * 4, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST),
            w,
            h,
        }
    }

    /// Develops the viewport at (x0, y0) and copies the RGBA8 pixels into `dst` (w * h * 4 bytes).
    /// Returns (GPU milliseconds, readback milliseconds).
    pub fn render_view(&self, vp: &ViewPath, x0: u32, y0: u32, dst: &mut [u8]) -> Result<(f64, f64)> {
        let p0 = self.scene_params;
        let p = Params { tile_x: x0, tile_y: y0, tile_w: vp.w, tile_h: vp.h, in_x0: 0, in_y0: 0, in_stride: vp.w, out_x0: 0, out_y0: 0, out_stride: vp.w, ..p0 };
        let t0 = Instant::now();
        let mut enc = self.encoder();
        self.demosaic_pass(&mut enc, &p, &vp.inter);
        self.pass(&mut enc, &self.tone, &p, &[&vp.inter, &self.lut, &vp.out]);
        enc.copy_buffer_to_buffer(&vp.out, 0, &vp.readback, 0, Some(vp.w as u64 * vp.h as u64 * 4));
        self.g.queue.submit([enc.finish()]);
        self.g.wait()?;
        let gpu_ms = ms(t0.elapsed());
        let t1 = Instant::now();
        let slice = vp.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.g.wait()?;
        rx.recv()?.map_err(|e| anyhow!("map: {e:?}"))?;
        {
            let data = slice.get_mapped_range().map_err(|e| anyhow!("range: {e:?}"))?;
            dst.copy_from_slice(&data);
        }
        vp.readback.unmap();
        Ok((gpu_ms, ms(t1.elapsed())))
    }
}

/// The stages of the full chain, in order. Changing a setting reruns its stage and every later one.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, serde::Serialize)]
pub enum Stage {
    Demosaic,
    Denoise,
    Blur,
    Combine,
    Tone,
}

pub const STAGES: [Stage; 5] = [Stage::Demosaic, Stage::Denoise, Stage::Blur, Stage::Combine, Stage::Tone];

/// Where a chain runs: the region of the image it develops (with a halo around the part that is
/// kept) and where the kept part goes in the output buffer.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub x0: u32,
    pub y0: u32,
    pub rw: u32,
    pub rh: u32,
    pub halo_x: u32,
    pub halo_y: u32,
    pub inner_w: u32,
    pub inner_h: u32,
    pub out_x0: u32,
    pub out_y0: u32,
    pub out_stride: u32,
}

/// The buffers of a chain, sized for a region of `w` by `h` pixels.
pub struct Chain {
    pub a: wgpu::Buffer,
    den: wgpu::Buffer,
    t: wgpu::Buffer,
    small: wgpu::Buffer,
    large: wgpu::Buffer,
    comb: wgpu::Buffer,
    pub out: wgpu::Buffer,
}

impl<'g> Renderer<'g> {
    pub fn chain(&self, w: u32, h: u32, out_words: u64) -> Chain {
        let n = w as u64 * h as u64 * 16;
        let comb = self.storage(n);
        // The blur's temporary buffer is the combine output: the stages never need both at once.
        Chain { a: self.storage(n), den: self.storage(n), t: comb.clone(), small: self.storage(n), large: self.storage(n), comb, out: self.storage(out_words * 4) }
    }

    /// Like `chain`, but returns `None` instead of failing when the GPU is out of memory.
    pub fn try_chain(&self, w: u32, h: u32, out_words: u64) -> Option<Chain> {
        // One buffer of the chain must fit in a storage binding.
        if w as u64 * h as u64 * 16 > self.g.limits.max_storage_buffer_binding_size as u64 {
            return None;
        }
        let scope = self.g.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let c = self.chain(w, h, out_words);
        pollster::block_on(scope.pop()).is_none().then_some(c)
    }

    /// A chain whose camera RGB buffer is one that already exists (the fit-to-screen cache).
    pub fn chain_from(&self, a: wgpu::Buffer, w: u32, h: u32, out_words: u64) -> Chain {
        let n = w as u64 * h as u64 * 16;
        let comb = self.storage(n);
        Chain { a, den: self.storage(n), t: comb.clone(), small: self.storage(n), large: self.storage(n), comb, out: self.storage(out_words * 4) }
    }

    fn encode_stage(&self, enc: &mut wgpu::CommandEncoder, stage: Stage, c: &Chain, s: &Spec, base: &Params) {
        let region = Params { tile_x: s.x0, tile_y: s.y0, tile_w: s.rw, tile_h: s.rh, ..*base };
        match stage {
            Stage::Demosaic => self.demosaic_pass(enc, &region, &c.a),
            Stage::Denoise => self.pass(enc, &self.nlm, &region, &[&c.a, &c.den]),
            Stage::Blur => {
                let with = |radius: f32, dir: u32| Params { blur: [radius as u32, dir, 0, 0], ..region };
                let (rs, rl) = (base.op[1][1], base.op[2][1]);
                self.pass(enc, &self.blur, &with(rs, 0), &[&c.den, &c.t]);
                self.pass(enc, &self.blur, &with(rs, 1), &[&c.t, &c.small]);
                self.pass(enc, &self.blur, &with(rl, 0), &[&c.den, &c.t]);
                self.pass(enc, &self.blur, &with(rl, 1), &[&c.t, &c.large]);
            }
            Stage::Combine => self.pass(enc, &self.combine, &region, &[&c.den, &c.small, &c.large, &c.comb]),
            Stage::Tone => {
                let tone = Params {
                    tile_w: s.inner_w,
                    tile_h: s.inner_h,
                    in_x0: s.halo_x,
                    in_y0: s.halo_y,
                    in_stride: s.rw,
                    out_x0: s.out_x0,
                    out_y0: s.out_y0,
                    out_stride: s.out_stride,
                    ..*base
                };
                self.pass(enc, &self.tone, &tone, &[&c.comb, &self.lut, &c.out]);
            }
        }
    }

    /// Runs every stage from `from` to the end in one submission. Returns milliseconds.
    pub fn chain_run(&self, c: &Chain, s: &Spec, base: &Params, from: Stage) -> Result<f64> {
        let start = Instant::now();
        let mut enc = self.encoder();
        for stage in STAGES.into_iter().filter(|st| *st >= from) {
            self.encode_stage(&mut enc, stage, c, s, base);
        }
        self.submit_wait(enc)?;
        Ok(ms(start.elapsed()))
    }

    /// Runs one stage alone. Returns milliseconds.
    pub fn chain_stage(&self, c: &Chain, s: &Spec, base: &Params, stage: Stage) -> Result<f64> {
        let start = Instant::now();
        let mut enc = self.encoder();
        self.encode_stage(&mut enc, stage, c, s, base);
        self.submit_wait(enc)?;
        Ok(ms(start.elapsed()))
    }

    /// Full-resolution export through the whole chain, by full-width bands with a halo.
    pub fn export_chain(&self, base: &Params, halo: u32, budget_mb: u64) -> Result<(Vec<u32>, f64, u32)> {
        let (w, h) = (base.width, base.height);
        let budget = budget_mb << 20;
        let max_rh = ((budget / (5 * 16 * w as u64)) as u32).clamp(64, 2048);
        let inner = max_rh.saturating_sub(2 * halo).max(16);
        let c = self.chain(w, inner + 2 * halo, w as u64 * inner as u64);
        let readback = Self::make(self.g, w as u64 * h as u64 * 4, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
        let start = Instant::now();
        let mut y0 = 0;
        while y0 < h {
            let th = inner.min(h - y0);
            let top = halo.min(y0);
            let bottom = halo.min(h - (y0 + th));
            let spec = Spec { x0: 0, y0: y0 - top, rw: w, rh: th + top + bottom, halo_x: 0, halo_y: top, inner_w: w, inner_h: th, out_x0: 0, out_y0: 0, out_stride: w };
            let mut enc = self.encoder();
            for stage in STAGES {
                self.encode_stage(&mut enc, stage, &c, &spec, base);
            }
            enc.copy_buffer_to_buffer(&c.out, 0, &readback, y0 as u64 * w as u64 * 4, Some(th as u64 * w as u64 * 4));
            self.g.queue.submit([enc.finish()]);
            y0 += th;
        }
        self.g.wait()?;
        let elapsed = ms(start.elapsed());
        Ok((read_words(self.g, &readback)?, elapsed, inner))
    }
}

fn read_words(g: &Gpu, buf: &wgpu::Buffer) -> Result<Vec<u32>> {
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    g.wait()?;
    rx.recv()?.map_err(|e| anyhow!("map: {e:?}"))?;
    let data = slice.get_mapped_range().map_err(|e| anyhow!("range: {e:?}"))?;
    let v = bytemuck::cast_slice::<u8, u32>(&data).to_vec();
    drop(data);
    buf.unmap();
    Ok(v)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
