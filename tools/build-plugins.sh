#!/usr/bin/env bash
# Builds the WebAssembly plugins in plugins/ (a separate workspace, WP6) that
# auroraw-plugin-host's tests load: plugins/target/wasm32-wasip1/release/*.wasm.
# Needs the wasm32-wasip1 target (rust-toolchain.toml installs it via `rustup show`).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --target wasm32-wasip1 --manifest-path plugins/Cargo.toml --locked
