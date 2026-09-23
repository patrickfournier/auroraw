// SPDX-License-Identifier: GPL-3.0-or-later
//! A [`Decoder`] backed by a sandboxed WebAssembly plugin: `rawler-decoder`, the productized
//! `rawler` decoder (architecture §8.1, "the first real WebAssembly plugin"), and the wire
//! protocol `plugins/rawler-decoder` implements on the guest side of the same C interface
//! [`crate::plugin`] speaks on the host side.

use crate::grants::Grants;
use crate::plugin::{CompiledPlugin, PluginHost};
use auroraw_plugin_api::{Decoder, DecoderError, RawImage};
use std::sync::Arc;
use std::time::Duration;

/// The `out` buffer's size: seven little-endian `u32` values (width, height, components per
/// pixel, the samples' address, their count, black level, white level), matching
/// `plugins/rawler-decoder`'s own doc comment.
const HEADER_SIZE: usize = 28;

/// A memory ceiling generous enough for the largest embedded mosaic seen so far (spike 4: up to
/// 347 MB of guest memory for a 60 MP file) with real headroom, since a decode that hits the
/// ceiling fails that file rather than the host.
const DEFAULT_MEMORY_LIMIT: usize = 512 << 20;

/// How long a single decode may run before the host interrupts it. Generous: a slow decode is
/// still a real one (spike 4's own slowest sample took over a second natively), and this guards
/// against a hang, not against a merely slow file.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// [`Decoder`] implemented by `plugins/rawler-decoder` running under [`PluginHost`]. A fresh
/// instance is started for every call (spike 4's own measurement shape: instantiating from an
/// already-compiled module costs 0.07 ms), so calling [`Decoder::decode`] from several threads at
/// once decodes several files in parallel, each in its own sandbox.
pub struct WasmDecoder {
    host: Arc<PluginHost>,
    plugin: CompiledPlugin,
}

impl WasmDecoder {
    /// Compiles `wasm` (the bytes of `rawler-decoder`'s `.wasm` file) against `host`.
    pub fn new(host: Arc<PluginHost>, wasm: &[u8]) -> crate::error::Result<Self> {
        let plugin = host.load(wasm)?;
        Ok(Self { host, plugin })
    }
}

impl Decoder for WasmDecoder {
    fn decode(&self, bytes: &[u8]) -> Result<RawImage, DecoderError> {
        let grants = Grants {
            memory_limit: Some(DEFAULT_MEMORY_LIMIT),
            ..Grants::default()
        };
        let mut instance = self
            .host
            .instantiate(&self.plugin, &grants)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;

        let at = instance
            .alloc(bytes.len())
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        instance
            .write(at, bytes)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        let out_at = instance
            .alloc(HEADER_SIZE)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;

        let rc: i32 = instance
            .call("import", (at, bytes.len() as u32, out_at), DEFAULT_TIMEOUT)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        if rc != 0 {
            return Err(DecoderError::Invalid(format!(
                "the decoder plugin refused this file (code {rc})"
            )));
        }

        let mut header = [0u8; HEADER_SIZE];
        instance
            .read(out_at, &mut header)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        let words: Vec<u32> = header
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().expect("4-byte chunks")))
            .collect();
        let (width, height, components_per_pixel, samples_ptr, samples_len, black_bits, white_bits) = (
            words[0], words[1], words[2], words[3], words[4], words[5], words[6],
        );

        let mut raw = vec![0u8; samples_len as usize * 2];
        instance
            .read(samples_ptr, &mut raw)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        let samples: Vec<u16> = raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();

        // The instance and its memory are dropped here: no explicit `release` call needed, since
        // nothing outlives this function that still points into the plugin's memory (unlike the
        // spike's own long-lived benchmark instances, which called it to reuse one instance for
        // many files).
        Ok(RawImage {
            width,
            height,
            components_per_pixel,
            samples,
            black_level: f32::from_bits(black_bits),
            white_level: f32::from_bits(white_bits),
        })
    }
}
