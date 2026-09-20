# Spike 1: the GPU pipeline (interim report)

> **Status: interim.** Measured on Linux with a real GPU, and on Linux, Windows and macOS
> through continuous integration (no discrete GPU there), all with synthetic data. Real RAW
> files and real Windows and macOS GPUs are still to come (see "What remains"). Nothing here is a
> final decision.

## Question

Can one Rust pipeline on wgpu turn a RAW into a colour-managed image fast enough, with the same
result on Vulkan, Metal and DirectX 12, and with a CPU fallback? (docs/technical-spikes.md, §2)

## What was built

`spikes/gpu-pipeline`, about 700 lines of Rust and WGSL:

- Three compute shaders: **demosaicing** (Malvar-He-Cutler gradient-corrected interpolation,
  with black level and white balance), **box downscale**, and **tone** (camera RGB to a wide
  gamut working space, exposure, a tone curve, working space to display primaries, a 33x33x33
  LUT standing in for an ICC display profile, sRGB encoding, 8-bit packing).
- A **CPU reference** with the same maths in f32, multi-threaded (rayon).
- Three paths measured: a **full-resolution export** by full-width bands, a **100% view** (a
  2560x1440 viewport), and a **fit-to-screen view**, each with the camera RGB cached at its
  stage boundary so that a downstream change reruns only the tone pass.
- A benchmark that prints every measure as JSON.

The input is a synthetic Bayer mosaic (smooth colour field, texture, noise) because no RAW file
is available yet. The pipeline has no data-dependent branching, so the timings should transfer;
the accuracy result should be repeated on real files.

## Results

Machine: NVIDIA GeForce GTX 1650 SUPER (4 GB, Vulkan, driver 580.178), 16 CPU threads. The
software adapter is llvmpipe (Mesa 25.2, LLVM 20), which runs the very same shaders on the CPU.

| Measure (target) | GTX 1650 SUPER, 24 MP | GTX 1650 SUPER, 61 MP | llvmpipe, 24 MP |
| --- | --- | --- | --- |
| Full render, developed and tone-mapped | **27 ms** | **62 ms** | 235 ms |
| Read back to the CPU (96 MB / 241 MB) | 35 ms | 86 ms | 33 ms |
| 100% view, upstream change (demosaic and tone) | **1.2 ms** | 1.2 ms | 23.9 ms |
| 100% view, downstream change (tone only) (target: under 50 ms) | **0.6 ms** | 0.6 ms | 14.7 ms |
| Fit view, cold | 12 ms | 36 ms | 154 ms |
| Fit view, slider (target: under 50 ms) | **0.4 ms** | 0.6 ms | 11.9 ms |
| Accuracy against the CPU reference: worst difference | 1 level | 1 level | 1 level |
| Pixels differing by more than 1 level (target: under 0.1%) | 0% | 0% | 0% |
| CPU reference, 16 threads | 354 ms | 966 ms | 401 ms |

### The three platforms, through continuous integration

The GitHub runners have no discrete GPU, so these are the adapters they offer. They test that
the shaders run and agree, not that a real GPU is fast. All four have 3 to 4 CPU threads.

| Adapter | Full render, 24 MP | 100% view, tone only | Fit view, slider | Worst difference vs CPU | Rust CPU reference |
| --- | --- | --- | --- | --- | --- |
| macOS, **Metal**, Apple paravirtual GPU | 89 ms | 2.0 ms | 1.8 ms | 1 level | 1,094 ms |
| Linux, **Vulkan**, llvmpipe (software) | 677 ms | 57.9 ms | 42.7 ms | 1 level | 1,288 ms |
| Windows, **DirectX 12**, WARP (software) | 3,056 ms | 122.2 ms | 88.4 ms | 1 level | 1,130 ms |

In every case the fraction of pixels differing by more than one level is 0%.

Other facts:

- **Memory.** The 61 MP run used about 700 MB of GPU memory in the process. The desktop was
  already using about 2.3 GB of the 4 GB card.
- **Small binding limit.** llvmpipe limits a storage binding to 128 MiB, which forced the
  band size down (1,398 rows at 24 MP). The banded design worked there without change, so it
  will fit any GPU.
- **wgpu's GL back end** lost the device on this workload. It is not a target platform.
- **Intel UHD 630** is not visible to wgpu from this sandbox, so it could not be tested.

## What it means

1. **The architecture holds.** Caching the camera RGB at the stage boundary makes a downstream
   change cost about 0.6 ms at a 100% view, against a budget of 50 ms. An upstream change costs
   about 1.2 ms. A full 24 MP render is 27 ms.
2. **There is a lot of headroom, but this is a minimal pipeline.** The real one will add noise
   reduction, sharpening, local contrast, masks, lens correction and more, and heavy operations
   will eat into the headroom. The budget is safe for now, not proven for M3.
3. **Banding works** and is what makes 61 MP images fit small GPUs.
4. **The CPU fallback depends on the platform.** On Linux, the same shaders on llvmpipe were
   about 9 times slower than the GPU (target: under 20 times), gave the same result within one
   level, and were **faster** than a hand-written Rust CPU implementation (235 ms against 354 ms
   on 16 threads; 677 ms against 1,288 ms on 4). That favours the shared-shader strategy there.
   **On Windows the picture reverses:** WARP took 3,056 ms, nearly three times slower than the
   Rust CPU code on the same cores (1,130 ms), and its slider time (88 ms) misses the 50 ms
   target. macOS always has a GPU. In practice almost every Windows machine has a DirectX 12
   capable GPU, including integrated ones, so the fallback matters for old or virtual machines.
   Decision deferred: the choices are shared shaders everywhere (simple, slow on Windows
   without a GPU) or shared shaders plus a Rust CPU path used where the software adapter is too
   slow (faster, two implementations to keep equal).
5. **The four adapters agree.** NVIDIA and llvmpipe on Vulkan, Metal, and DirectX 12 all give
   the same image within one 8-bit level, on 0% of pixels beyond one level. This is the
   cross-platform result the spike was after, on the three graphics APIs.

## What remains

- [ ] **Real GPUs on Windows and macOS.** The workflow's binaries can be run on Patrick's
  machines to get real numbers. Correctness on both is already shown.
- [ ] **Real RAW files**, from several makers, decoded with a real decoder.
- [ ] **A heavier pipeline** (noise reduction, local contrast, a mask) to see where the budget
  starts to bind.
- [ ] **Presentation.** These numbers stop at the buffer: no window, no swap chain, no vertical
  sync. That belongs to spike 2.
- [ ] **A second GPU**, such as the Intel UHD 630, or an AMD card.
- [ ] **Sensors that are not Bayer.** The spike demosaics an RGGB mosaic only. Fujifilm's
  X-Trans and other patterns need their own demosaicing, which the pipeline must accommodate.

## Running it

```bash
cd spikes
cargo build --release
./target/release/adapters                                  # lists what wgpu sees
./target/release/bench --adapter "vulkan nvidia" --mp 24    # or 61; "dx12", "metal", ...
```

Raw results are in `spikes/results/`.
