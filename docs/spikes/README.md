# The technical spikes

Throwaway prototypes measured the technology choices (see [../technical-spikes.md](../technical-spikes.md)):

1. [The GPU pipeline](01-gpu-pipeline.md): wgpu, WGSL, the same result on three graphics APIs.
2. [The image view and the interface toolkit](02-image-view-and-toolkit.md): Slint, decision D-072.
3. [The catalogue, the thumbnails and the workspace](03-catalogue-and-grid.md): SQLite, D-073 to D-075.
4. [The plugin sandbox](04-plugin-sandbox.md): WebAssembly on wasmtime, D-076 to D-078.
5. [The interface slice on Qt Quick, through cxx-qt](05-qt-quick-through-rust.md): what a switch from Slint would cost (measured, no decision).

**The code** is no longer on `dev`. It is kept at the tag `spikes-final`, in `spikes/`:

```bash
git show spikes-final:spikes/Cargo.toml        # look at a file
git worktree add ../auroraw-spikes spikes-final # or check the whole tree out beside this one
```

**The raw results** (JSON and text files, including the Windows run of issue #1) are in
[results/](results/) and [results-windows/](results-windows/).

The harnesses that the [testing strategy](../testing-strategy.md) keeps are moved into the
product as their work packages come up: the smoke test and the golden-image tools with the
pipeline (M2), the dataset generator with the catalogue (WP2), the hostile plugins with the
plugin host (WP6). The samples script is [../../tools/fetch-samples.sh](../../tools/fetch-samples.sh)
and the accessibility script is [../../tools/a11y-check.py](../../tools/a11y-check.py).
