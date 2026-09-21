# Spike 4: the plugin sandbox (interim report)

> **Status: interim.** Measured on Linux. Windows and macOS runs of the sandbox tests are built by
> continuous integration and pending. Nothing here is a final decision.

## Question

Is WebAssembly a workable sandbox for image operations and importers, at what cost, and how do
GPU operations fit? (docs/technical-spikes.md, §5; decisions D-054 to D-057.)

## What was built

`spikes/plugin-host` (a host on **wasmtime 48**) and `spikes/plugins` (four plugins compiled to
`wasm32-wasip1`, in Rust):

- **`ops`**: an operation plugin. Image kernels (a tone curve with `powf` and `sqrt`, a 5x5
  convolution, a saturation change) behind a small C interface: the host reserves buffers in the
  plugin's memory, writes the pixels, calls, and reads the result.
- **`rawimport`**: an import plugin. **`rawler`** (the decoder used in spike 1) compiled to
  WebAssembly. The host hands it the file's bytes; the plugin never touches the file system. (The
  plan named LibRaw, which needs a C compiler for WebAssembly; `rawler` answers the same
  question without one.)
- **`gpuop`**: an operation plugin with a **GPU shader** and a CPU twin. It declares itself in
  JSON (identifier, version, stage, constraints, parameters, permissions) and ships WGSL as data.
- **`hostile`**: a plugin that misbehaves on purpose.

## Results

### Image operations in the sandbox

A tile of 2048x2048 pixels (4.2 MP, 64 MB as f32 RGBA), median of 5. "Native" is the same Rust
code compiled for the host with default flags.

| | Native | WebAssembly | Ratio |
| --- | --- | --- | --- |
| Tone (`powf` and `sqrt` per pixel): the run alone | 86 ms | 123 ms | **1.4x** |
| The same with the copy in and out (5.6 + 7.2 ms) | 86 ms | 136 ms | 1.6x |
| 5x5 convolution: the run alone | 157 ms | 190 ms | **1.2x** |
| The same with the copies | 157 ms | 202 ms | 1.3x |
| The same, built with **SIMD128** | 156 ms | **108 ms** | **0.7x** |
| A sparse pass over the memory | 3.4 ms | 3.6 ms | 1.0x |

The results are **bit-identical** to the native ones. The SIMD build is faster than the native
one because the native build targets a generic processor; a tuned native build would be faster
again. The cost of the boundary itself is small: a call that does nothing takes **40 ns**, and
copying a 64 MB tile in and out costs 12 ms, about a tenth of the work.

| | Cost |
| --- | --- |
| Compile a plugin (38 KB of WebAssembly) | 7.6 ms |
| Load the compiled copy from the cache | **0.08 ms** |
| Create an instance | 0.07 ms |
| Watching the plugin: epoch interruption / fuel metering, on the 5x5 convolution | +6% / +9% |

Sixteen tiles of 1 MP on N threads, each thread with its own instance:

| Threads | 1 | 2 | 4 | 8 | 16 |
| --- | --- | --- | --- | --- | --- |
| Native | 377 ms | 193 ms | 113 ms | 87 ms | 79 ms |
| WebAssembly | 543 ms | 292 ms | 174 ms | 120 ms | 123 ms |
| Ratio | 1.44 | 1.51 | 1.54 | 1.38 | 1.55 |

Plugin instances scale like native threads, at 1.4 to 1.55 times the time whatever the number
of threads.

### A decoder in the sandbox

`rawler` as an import plugin, on the six real RAW files, native and WebAssembly decoding to the
same 16-bit mosaic. The pixels are **identical in every case**.

| File | Size | Native, one thread | WebAssembly | Ratio | Guest memory |
| --- | --- | --- | --- | --- | --- |
| Sony A7R IV (ARW) | 59 MB | 258 ms | 367 ms | 1.4x | 261 MB |
| Fujifilm X-T50 (RAF) | 28 MB | 1,457 ms | 2,069 ms | 1.4x | 184 MB |
| Nikon D850 (NEF) | 45 MB | 250 ms | 347 ms | 1.4x | 243 MB |
| Olympus E-M5 III (ORF) | 17 MB | 252 ms | 326 ms | 1.3x | 115 MB |
| Canon R5 II (CR3) | 26 MB | 899 ms | 1,428 ms | 1.6x | 347 MB |
| Panasonic S5 (RW2) | 34 MB | 90 ms | 181 ms | 2.0x | 175 MB |

Handing over the file costs 6 to 19 ms, and getting the mosaic back 13 to 39 ms (up to 120 MB
at 60 MP). Compiling the 4.6 MB plugin takes 3 s, loading the compiled copy 4.7 ms.

**Against the native decoder running on all cores, the gap is larger**, up to 8.5 times for
Sony, Fujifilm, Canon and Panasonic files: those decoders use several threads natively and the
plugin runs on one. Like for like (native on one thread) it is 1.3 to 2.0 times, inside the
3 times target. Ways to recover the difference: decode several files at once, each in its own
instance (the instances scale), or give decoders the "native" level (D-055).

### Failures and hostile plugins

Each case runs in a fresh instance; after each one a normal plugin is run to prove the host is
unharmed (**"host alive: yes" in every case**).

| A plugin that... | What happens | Time |
| --- | --- | --- |
| panics | trap, the call returns an error | 1.0 ms |
| loops forever, with a 10 / 100 / 1,000 ms timer | interrupted | **10.2 / 100.1 / 1,000.2 ms** |
| loops forever, with 300 million steps of fuel | stopped | 20 ms |
| asks for 100 MB, limit 256 MB | gets it | 28 ms |
| asks for 2,000 MB, limit 256 MB | **refused** (the host's peak grew by 155 MB) | 69 ms |
| recurses endlessly | trap | 0.3 ms |

An infinite loop is stopped within 0.2 ms of the deadline set by the host.

| A plugin that tries to... | Nothing granted | One folder granted, read only |
| --- | --- | --- |
| read a private file of the host, or `/etc/passwd` | refused | refused |
| read a file in the granted folder | refused | **allowed** |
| read outside the folder with `../` | refused | **refused** |
| list the granted folder | refused | allowed |
| write a file in the granted folder | refused | **refused** (no file was created) |
| open a network connection | not available | not available |
| start a program | not available | not available |
| start a thread | not available | not available |
| read the environment variables | **0 seen** | 0 seen |
| read the clock | possible | possible |

The clock is available to every plugin. A plugin can therefore time things, which matters for
side channels but not for this use.

### A GPU operation plugin

`gpuop` declares itself in JSON: identifier, version, API version, family, panel, stage
(`scene-linear`), constraints (after `demosaic`, before `tone`), its parameter with limits and
default, and **no permissions**. The shader is data.

- **Placement from the declaration**: the host put it between the local contrast and the tone
  stage, as late as its constraints allow. A declaration that cannot be satisfied (after the
  output and before the demosaic) and one naming an unknown stage are **refused with a reason**.
- **Compiled and validated in 5.4 ms**; **0.67 ms on the GPU** for a 1920x1080 region (GTX 1650
  SUPER); the same operation as the plugin's CPU twin in the sandbox takes 9.5 ms.
- **The two agree**: worst absolute difference 1.5e-8, worst relative 7.5e-6, on values
  that the operation changed for 71% of them.
- **Refused, with the reason and without a crash**: a syntax error ("expected operation `=`,
  found `==`"), a store into a read-only buffer, a missing entry point, an undeclared variable,
  an unbounded loop, and a workgroup of 1,024 invocations.

The GPU is the weak point. **A shader that never ends cannot be stopped on a GPU** the way a
WebAssembly loop can: it can freeze the display. The host's check refuses `loop` and `while`
and limits the workgroup; that is crude, and a real check must prove that every loop is bounded.
Also, a shader can be valid for the driver on Linux and rejected by DirectX's compiler (spike 1),
so a portability check must run on every platform before a plugin enters the index.

## What it means

1. **WebAssembly works as the sandbox** (D-055). Cost: 1.2 to 2 times native, bit-identical
   results, instances that scale with threads, and near-zero call overhead (40 ns).
2. **The sandbox holds** on every case tried, with permissions that are granted explicitly: no
   files, no network, no environment, no processes unless the host allows it, with per-plugin
   limits on memory and time, and the host never harmed.
3. **A compiled cache makes plugins cheap to start**: compiling 3 s or 8 ms, loading 5 ms or
   0.08 ms. Compile at installation, keep the result.
4. **Decoders are where the sandbox hurts**: they are the hot spot, they use threads natively,
   and they need 100 to 350 MB. A native level for decoders (D-055) remains justified, with
   WebAssembly as the default and decode-many-files-in-parallel as the mitigation.
5. **GPU plugins need a stronger check than CPU ones**, because they cannot be interrupted.
6. **The declaration works**: placement in a configurable pipeline (§5.6) can be done from a
   declared stage and constraints, and impossible declarations are refused.
7. **Memory per instance is real**: a decoder instance holds up to 350 MB of guest memory; a
   limit is enforced, and at 2,000 MB requested the host still had to touch 155 MB before
   refusing.

## What remains

- [x] **Windows and macOS, in continuous integration.** Run 35535924382 (commit 57eca26) passed the
  sandbox tests on both: every hostile case left the host alive, on Windows too (panic, loops
  stopped by the timer and by fuel, the memory limit). Timings from shared runners are not
  comparable with the Linux figures and are not recorded here.
- [ ] **A real Windows machine** would still confirm the permission behaviour (paths, file access
  rules) on the granted-folder cases.
- [ ] **The component model.** These plugins use a hand-made C interface. wasmtime's component model
  and WIT would give typed interfaces; its cost was not measured.
- [ ] **WebAssembly threads**, which would recover the parallel decoders, are not enabled.
- [ ] **LibRaw** as a plugin, needing wasi-sdk.
- [ ] **A GPU shader safety check** that proves loops bounded, and the portability check.
- [ ] **Distribution and signing** of plugins (the index of D-054): not touched.

## Running it

```bash
cd spikes/plugins && cargo build --release --target wasm32-wasip1      # needs the wasm32-wasip1 target
RUSTFLAGS="-C target-feature=+simd128" cargo build --release --target wasm32-wasip1 -p ops --target-dir target-simd
cd .. && cargo build --release
./target/release/opbench      # operations, natively against WebAssembly
./target/release/sandbox      # crashes, loops, memory, permissions
./target/release/importbench samples     # the RAW decoder plugin (RAYON_NUM_THREADS=1 for the native baseline)
./target/release/gpubench     # the GPU operation plugin
```

Raw results are in `spikes/results/spike4-*.json`.
