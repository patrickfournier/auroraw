// SPDX-License-Identifier: GPL-3.0-or-later
//! A perceptual hash for visual similarity (spec §5.3, WP9's near-duplicate suggestions): built
//! here because it falls out of a thumbnail almost for free, not because this crate does
//! anything with it. WP9 decides what "close" means and what a person is offered because of it.

use image::DynamicImage;
use image::imageops::FilterType;

/// A 64-bit difference hash: shrink to 9x8 grayscale and record, for each of the 8x8 pixels,
/// whether it is brighter than its right neighbour. Two photos of the same scene hash a short
/// Hamming distance apart even after a recompress or a small crop; two different scenes usually
/// do not. A simple, well-known, and cheap algorithm — accuracy tuning is WP9's job once it has
/// a real photo library to tune against.
pub fn perceptual_hash(image: &DynamicImage) -> u64 {
    let small = image.resize_exact(9, 8, FilterType::Triangle).to_luma8();
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            let left = small.get_pixel(x, y).0[0];
            let right = small.get_pixel(x + 1, y).0[0];
            hash = (hash << 1) | u64::from(left > right);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn gradient(w: u32, h: u32, reverse: bool) -> DynamicImage {
        let buf = ImageBuffer::from_fn(w, h, |x, _| {
            let v = (x * 255 / w.max(1)) as u8;
            Rgb([if reverse { 255 - v } else { v }; 3])
        });
        DynamicImage::ImageRgb8(buf)
    }

    fn flat(w: u32, h: u32, value: u8) -> DynamicImage {
        DynamicImage::ImageRgb8(ImageBuffer::from_pixel(w, h, Rgb([value; 3])))
    }

    #[test]
    fn the_same_image_always_hashes_the_same() {
        let image = gradient(64, 48, false);
        assert_eq!(perceptual_hash(&image), perceptual_hash(&image));
    }

    #[test]
    fn a_flat_image_has_no_bit_set() {
        // Every neighbour comparison is equal, never "brighter than", so every bit is 0.
        assert_eq!(perceptual_hash(&flat(64, 48, 128)), 0);
    }

    #[test]
    fn very_different_images_hash_far_apart() {
        let a = gradient(64, 48, false);
        let b = gradient(64, 48, true);
        let distance = (perceptual_hash(&a) ^ perceptual_hash(&b)).count_ones();
        assert!(
            distance > 32,
            "expected a large Hamming distance, got {distance}"
        );
    }
}
