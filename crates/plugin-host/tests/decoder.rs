// SPDX-License-Identifier: GPL-3.0-or-later
//! The WebAssembly `rawler-decoder` plugin against the same decoder running natively: the pixels
//! must be identical (architecture §8.1: "the first real WebAssembly plugin, loaded through the
//! host, and the tests prove it returns exactly what the native one does", spike 4's own check).
//!
//! Skipped locally when the plugin has not been built (`tools/build-plugins.sh`) or the samples
//! have not been fetched (`tools/fetch-samples.sh`); both are required in CI
//! (`AUR_REQUIRE_PLUGINS=1`, `AUR_REQUIRE_SAMPLES=1`), matching `imaging`'s own pattern for
//! `AUR_REQUIRE_EXIFTOOL`.

use auroraw_plugin_api::Decoder;
use auroraw_plugin_host::{PluginHost, WasmDecoder};
use rawler::RawImageData;
use rawler::decoders::RawDecodeParams;
use rawler::rawsource::RawSource;
use std::path::PathBuf;
use std::sync::Arc;

const SAMPLES: &[&str] = &[
    "B13A0732.CR2",
    "canon-r5m2-CRAW.CR3",
    "Nikon-D850-14bit-compressed.NEF",
    "DSC00396.ARW",
    "DSCF0120.RAF",
    "dc-s5_6k4k.RW2",
    "PB290154.ORF",
    "L1049390.DNG",
];

fn samples_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/samples");
    if dir.join(SAMPLES[0]).is_file() {
        return Some(dir);
    }
    if std::env::var_os("AUR_REQUIRE_SAMPLES").is_some() {
        panic!("testdata/samples is required here but was not found; run tools/fetch-samples.sh");
    }
    eprintln!("testdata/samples not found: skipping (run tools/fetch-samples.sh)");
    None
}

fn plugin_wasm() -> Option<Vec<u8>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/target/wasm32-wasip1/release/rawler_decoder.wasm");
    if path.is_file() {
        return Some(std::fs::read(&path).expect("read the compiled plugin"));
    }
    if std::env::var_os("AUR_REQUIRE_PLUGINS").is_some() {
        panic!(
            "rawler_decoder.wasm is required here but was not built; run tools/build-plugins.sh"
        );
    }
    eprintln!("rawler_decoder.wasm not found: skipping (run tools/build-plugins.sh)");
    None
}

#[test]
fn the_wasm_decoder_matches_the_native_one_on_every_sample() {
    let (Some(dir), Some(wasm)) = (samples_dir(), plugin_wasm()) else {
        return;
    };
    let host = Arc::new(PluginHost::new().unwrap());
    let decoder = WasmDecoder::new(host, &wasm).unwrap();

    for name in SAMPLES {
        let bytes = std::fs::read(dir.join(name)).unwrap();

        let native_source = RawSource::new_from_slice(&bytes);
        let native = rawler::decode(&native_source, &RawDecodeParams::default())
            .unwrap_or_else(|e| panic!("{name}: native decode failed: {e}"));
        let RawImageData::Integer(native_samples) = native.data else {
            panic!("{name}: native decode did not return integer samples");
        };

        let wasm_image = decoder
            .decode(&bytes)
            .unwrap_or_else(|e| panic!("{name}: WebAssembly decode failed: {e}"));

        assert_eq!(wasm_image.width, native.width as u32, "{name}: width");
        assert_eq!(wasm_image.height, native.height as u32, "{name}: height");
        assert_eq!(
            wasm_image.components_per_pixel, native.cpp as u32,
            "{name}: components per pixel"
        );
        assert_eq!(
            wasm_image.samples, native_samples,
            "{name}: the WebAssembly plugin's pixels differ from the native decoder's"
        );
    }
}
