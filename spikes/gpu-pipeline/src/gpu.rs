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
        let words: Vec<u32> = scene
            .mosaic
            .chunks(2)
            .map(|c| c[0] as u32 | ((*c.get(1).unwrap_or(&0) as u32) << 16))
            .collect();
        let mosaic = Self::make(g, (words.len() * 4) as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
        g.queue.write_buffer(&mosaic, 0, bytemuck::cast_slice(&words));
        let lut = Self::make(g, (scene.lut.len() * 16) as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
        g.queue.write_buffer(&lut, 0, bytemuck::cast_slice(&scene.lut));
        g.queue.submit([]);
        Ok(Self { g, demosaic, tone, downscale, mosaic, lut, scene_params: scene.params })
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
            self.pass(&mut enc, &self.demosaic, &p, &[&self.mosaic, &inter]);
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
        self.pass(&mut enc, &self.demosaic, &p, &[&self.mosaic, inter]);
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

    /// Fit-to-screen, cold: develops the whole image by bands, box-downscales it into `small`,
    /// then runs the tone pass. Returns (factor, small width, small height, milliseconds).
    pub fn fit_cold(&self, target_w: u32, small: &mut Option<wgpu::Buffer>, out: &mut Option<wgpu::Buffer>) -> Result<(u32, u32, u32, f64)> {
        let p0 = self.scene_params;
        let (w, h) = (p0.width, p0.height);
        let f = w.div_ceil(target_w).max(1);
        let (dw, dh) = (w.div_ceil(f), h.div_ceil(f));
        let rows = self.band_rows(w, f);
        let inter = self.storage(w as u64 * rows as u64 * 16);
        let small_buf = self.storage(dw as u64 * dh as u64 * 16);
        let out_buf = self.storage(dw as u64 * dh as u64 * 4);
        let start = Instant::now();
        let mut y0 = 0;
        while y0 < h {
            let th = rows.min(h - y0);
            let demosaic = Params { tile_x: 0, tile_y: y0, tile_w: w, tile_h: th, ..p0 };
            let down = Params { tile_w: dw, tile_h: th.div_ceil(f), src_w: w, src_h: th, factor: f, out_y0: y0 / f, ..p0 };
            let mut enc = self.encoder();
            self.pass(&mut enc, &self.demosaic, &demosaic, &[&self.mosaic, &inter]);
            self.pass(&mut enc, &self.downscale, &down, &[&inter, &small_buf]);
            self.g.queue.submit([enc.finish()]);
            y0 += th;
        }
        let tone = Params { tile_w: dw, tile_h: dh, in_x0: 0, in_y0: 0, in_stride: dw, out_x0: 0, out_y0: 0, out_stride: dw, ..p0 };
        let mut enc = self.encoder();
        self.pass(&mut enc, &self.tone, &tone, &[&small_buf, &self.lut, &out_buf]);
        self.submit_wait(enc)?;
        let t = ms(start.elapsed());
        *small = Some(small_buf);
        *out = Some(out_buf);
        Ok((f, dw, dh, t))
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
