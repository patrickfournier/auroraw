# Technical spikes

> **Status: spikes 1 and 2 done, 3 and 4 measured on Linux.** Phase 2 (development process) begins with the technology
> stack. Four choices depend on facts we do not have yet, so we measure before we decide.
> A spike is a small, throwaway prototype that answers one question with numbers. Its code is
> not part of the product.

## 1. Where we stand

Decided so far for the stack (see [decisions.md](decisions.md), D-068 to D-071):

- The core is written in **Rust** (catalogue, pipeline, plugin host).
- The interface priorities are **the performance and colour fidelity of the image view**, and
  **lightness** (fast start, little memory).
- Linux is tested here, Windows by Patrick, macOS by Patrick over remote desktop; continuous
  integration builds all three.
- The stack is fixed **after the spikes**, on measurements.
- **Spike 1 done**: wgpu is confirmed (docs/spikes/01-gpu-pipeline.md).
- **Spike 2 done**: the interface toolkit is **Slint**, D-072 (docs/spikes/02-image-view-and-toolkit.md).
- **Spike 3 measured on Linux**: SQLite meets every budget; a Windows run is pending (docs/spikes/03-catalogue-and-grid.md).
- **Spike 4 measured on Linux**: WebAssembly works as the plugin sandbox, 1.2 to 2 times native (docs/spikes/04-plugin-sandbox.md).

Candidates carried into the spikes:

| Area | Candidates | Notes |
| --- | --- | --- |
| GPU | **wgpu** (Vulkan, Metal, DirectX 12) with WGSL shaders | Direct Vulkan plus MoltenVK, and one back end per platform, are the fallbacks if wgpu falls short. |
| Interface | **Slint** (Rust, GPL-3.0 option), **Qt Quick** (through a Rust binding or a thin C++ layer), **iced** (built on wgpu) | A web view (Tauri) is not a candidate unless all the native ones fail: it goes against both stated priorities. |
| Catalogue | **SQLite** (rusqlite) | Not in question; the spike measures the schema and the queries. |
| Plugin sandbox | **WebAssembly** (wasmtime) for operations and importers, WGSL as data for GPU operations | The "native" level (D-055) is a separate, flagged path. |
| RAW decoding | **LibRaw** through bindings, against a pure-Rust decoder (rawler) | Coverage of camera models decides. |
| Colour | **lcms2** or a pure-Rust engine (moxcms) | Accuracy and speed. |

## 2. Spike 1: the GPU pipeline

**Question.** Can one Rust pipeline on wgpu turn a RAW into a colour-managed image, fast enough,
with the same result on Vulkan, Metal and DirectX 12, and with a CPU fallback?

**What is built.** A command-line tool: decode a RAW, demosaic, white balance, camera-to-working
space matrix (linear, wide gamut), exposure and a tone curve, a display transform through an ICC
profile. Compute shaders in WGSL, working on tiles.

**Measures and pass criteria** (from the performance budgets in the specification, §9):

| Measure | Target |
| --- | --- |
| Slider to pixel, fit-to-screen preview, 24 MP RAW, GTX 1650 SUPER | Under 50 ms |
| Same at 100% crop | Under 50 ms |
| Full-resolution render for export, 24 MP | Under 2 s |
| A 61 MP RAW on a 4 GB GPU | Works, by tiling, without failing |
| The same shaders on the Intel UHD 630 | Works |
| GPU against CPU fallback: maximum difference | 1 level in 8 bits on at least 99.9% of pixels |
| CPU fallback speed | Under 20 times slower than the GPU |
| Vulkan, Metal, DirectX 12: maximum difference | Same tolerance as above |

**CPU fallback strategies compared.** (a) The same shaders on a software back end (lavapipe on
Linux, WARP on Windows), which guarantees identical results; (b) separate Rust implementations,
which are faster but must be kept equivalent. macOS always has a GPU.

**Decides.** wgpu against the alternatives, the tile size, the CPU fallback strategy, and how
much of the AI-noise-reduction question (specification §10, 38) is a memory question.

## 3. Spike 2: the image view and the interface toolkit

**Question.** Which toolkit gives the smoothest, most colour-accurate image view with the least
weight, while still being able to build the rest of the application?

**What is built.** For each candidate, the same small application: a window with an image view
fed by the spike 1 pipeline (zoom and pan to 100%, side-by-side comparison), a virtualised
grid, a tree (for the keyword vocabulary), a text field with input method support, and a
language switch.

**Measures and pass criteria:**

| Measure | Target |
| --- | --- |
| Pan and zoom at 100%, 24 MP | 60 frames per second, no stutter |
| Photo shown from the preview cache | Under 100 ms |
| Colour | The test chart matches the display's own profile; wide gamut displays handled |
| Cold start to a usable window | Under 2 s |
| Memory with the view and a 100,000-item grid | Under 300 MB, excluding the image being edited |
| Grid, tree, drag and drop, text input, languages | All buildable without fighting the toolkit |
| Accessibility | A screen reader can reach the controls (a smoke test) |
| Package size | Recorded |

**Colour on macOS.** A remote desktop session does not show the real display's colours and may
report the wrong display profile, so colour is judged on Linux and Windows. On macOS the spike
checks that it runs, its speed and its behaviour, not the colours.

**Decides.** The interface toolkit, and how the pipeline's output reaches the screen.

## 4. Spike 3: the catalogue and the grid

**Question.** Does a SQLite catalogue with a thumbnail cache meet the budgets at 100,000 photos?

**What is built.** A generated catalogue of 100,000 photos with realistic metadata, keywords
(hierarchical), series and versions; a thumbnail generator and cache; the grid from spike 2.

**Measures and pass criteria:**

| Measure | Target |
| --- | --- |
| Opening the catalogue, cold | A few seconds at most |
| Search across 100,000 photos | Under 200 ms |
| Scrolling 10,000 thumbnails | No perceptible stutter |
| Thumbnail generation throughput | Recorded, per core |
| Thumbnail storage | Files against database blobs, size and speed compared |
| Rebuilding the catalogue from a workspace of 100,000 sidecars | Recorded |

**Decides.** The schema approach, the thumbnail storage, and whether the rebuild from the
workspace (D-026) is fast enough to be a routine operation.

## 5. Spike 4: the plugin sandbox

**Question.** Is WebAssembly a workable sandbox for image operations and importers, at what
cost, and how do GPU operations fit?

**What is built.** A host with wasmtime running (a) an operation plugin that processes tiles
of pixels, (b) an importer plugin, LibRaw compiled to WebAssembly, and (c) a GPU operation
plugin that ships WGSL and parameters and is placed in the pipeline from its declaration
(specification §5.6).

**Measures and pass criteria:**

| Measure | Target |
| --- | --- |
| Call overhead for a 4 MP tile | Small next to the work itself, recorded |
| LibRaw in WebAssembly against native | Within 3 times the native time |
| A plugin denied file or network access | Is stopped, shown by a test |
| A plugin that crashes or loops | Does not take the host down |
| A GPU operation plugin | Validated, placed and run without host code changes |

**Decides.** Whether WebAssembly is the sandbox (and which RAW decoders must sit at the
"native" level instead), and the shape of the plugin interface for the M2 internal modules.

## 6. Logistics

- **Code.** Each spike is its own crate under `spikes/` on the `dev` branch. It is throwaway and
  removed, or archived, once the decision is recorded.
- **Windows and macOS.** Continuous integration (GitHub Actions) builds each spike on Linux,
  Windows and macOS. Patrick downloads the binaries, runs them on the machines he has access to
  and sends the results. Each spike prints its measures as a small JSON file so the results are
  comparable.
- **Sample files.** Test RAW files from several makers are needed. The CC0 samples of
  raw.pixls.us cover most makers. Downloading them will be asked for first, with names and sizes.
- **Reports.** Each spike ends with a short report in `docs/spikes/` giving the measures, what
  they mean, and a recommendation.

## 7. Order

1. **Spike 1** first, since spikes 2 and 3 reuse its pipeline.
2. **Spikes 2 and 3** next, in parallel, the grid of spike 3 being part of the toolkit test.
3. **Spike 4** is independent and can run at any time.

The stack decision is then recorded in a decision, and the architecture document follows.

## 8. What the spikes do not settle

The catalogue and sidecar formats, the pipeline definition language, the AI runtime and the
licensing of plugins are decisions of their own (specification §10, technical phase). The
spikes only make sure the stack can carry them.
