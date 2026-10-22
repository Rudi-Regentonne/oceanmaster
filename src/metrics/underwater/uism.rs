//! Underwater image sharpness measure (UISM).
//!
//! Panetta et al., section III-B, equations (6) and (7). Sharpness is the
//! enhancement measure (EME) of a grayscale edge map, averaged over the
//! three color channels with the Rec. 601 weights:
//!
//! ```text
//! UISM = λ_R * EME(edge_R) + λ_G * EME(edge_G) + λ_B * EME(edge_B)
//! λ_R = 0.299,  λ_G = 0.587,  λ_B = 0.114
//! EME  = 2 / (k1 * k2) * Σ log(Imax / Imin)
//! ```
//!
//! How the edge map is built, from the paper:
//!
//! 1. The Sobel operator is applied to each RGB channel. Magnitude is
//!    `sqrt(gx² + gy²)` with replicated borders.
//! 2. That magnitude is multiplied by the original channel. The paper calls
//!    the product the grayscale edge map: edge pixels keep a value, and the
//!    product is what EME sees.
//! 3. "Only the pixels on the edges are preserved." A literal min/max of the
//!    product is undefined in almost every natural tile, because a smooth
//!    pixel has Sobel magnitude 0 and the ratio `Imax / Imin` divides by
//!    zero. Those zeros are therefore not treated as edge pixels. A pixel
//!    counts only when its Sobel magnitude is at least [`UismParams::edge_floor`]
//!    (default `1.0` on the 0–255 Sobel scale) and the product is positive.
//!    A tile with fewer than two such pixels contributes nothing.
//!
//! The divisor remains the full tile count `k1 * k2`, as in equation (7).
//! The paper uses 8×8 tiles for the numbers it reports. A strip that is not
//! a whole tile is left out rather than resized.

use image::RgbImage;

use crate::color::{channel, sobel};
use crate::config::UismParams;
use crate::error::Result;
use crate::metrics::eme::eme_from_pairs;

/// Sharpness of an 8-bit RGB image.
pub fn uism(image: &RgbImage, params: &UismParams) -> Result<f64> {
    params.validate()?;
    if image.width() == 0 || image.height() == 0 {
        return Err(crate::error::Error::EmptyImage);
    }
    let width = image.width();
    let height = image.height();
    let weights = [params.lambda_r, params.lambda_g, params.lambda_b];
    let mut score = 0.0;
    for (index, weight) in weights.iter().copied().enumerate() {
        let plane = channel(image, index);
        let gradient = sobel(&plane, width, height);
        let mut edge = Vec::with_capacity(plane.len());
        for (intensity, magnitude) in plane.iter().zip(gradient.magnitude.iter()) {
            edge.push(intensity * magnitude);
        }
        let floor = params.edge_floor as f32;
        let eme = eme_from_pairs(width, height, params.block_size, |x, y, bw, bh| {
            edge_min_max(
                &edge,
                &gradient.magnitude,
                width,
                Tile { x, y, w: bw, h: bh },
                floor,
            )
        })?;
        score += weight * eme;
    }
    Ok(score)
}

struct Tile {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

fn edge_min_max(
    values: &[f32],
    magnitude: &[f32],
    width: u32,
    tile: Tile,
    floor: f32,
) -> Option<(f32, f32)> {
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    let mut count = 0_u32;
    for row in tile.y..tile.y + tile.h {
        let start = (row * width + tile.x) as usize;
        for col in 0..tile.w as usize {
            let index = start + col;
            if magnitude[index] >= floor && values[index] > 0.0 {
                min_v = min_v.min(values[index]);
                max_v = max_v.max(values[index]);
                count += 1;
            }
        }
    }
    if count >= 2 && max_v > min_v {
        Some((min_v, max_v))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::UismParams;
    use image::{Rgb, RgbImage};

    #[test]
    fn a_flat_image_has_no_sharpness() {
        let image = RgbImage::from_pixel(32, 32, Rgb([30, 80, 90]));
        let score = uism(&image, &UismParams::default()).unwrap();
        assert_eq!(score, 0.0);
    }

    #[test]
    fn a_varying_edge_scores_above_zero() {
        // The edge itself changes brightness down the frame, so a tile that
        // contains it has a real max/min. A perfectly uniform step does not:
        // every edge pixel has the same product, and EME is undefined.
        let mut image = RgbImage::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let value = if x < 16 { 20 } else { 40 + (y as u8) * 6 };
                image.put_pixel(x, y, Rgb([value, value, value]));
            }
        }
        let score = uism(&image, &UismParams::default()).unwrap();
        assert!(
            score > 0.0,
            "a varying edge should produce UISM > 0, got {score}"
        );
    }
}
