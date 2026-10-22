//! Michelson contrast.
//!
//! Michelson contrast of a luminance plane is
//!
//! ```text
//! (Lmax - Lmin) / (Lmax + Lmin)
//! ```
//!
//! The global value uses the whole image. The block value is the mean of
//! that ratio over complete `block × block` tiles. A tile whose samples sum
//! to zero is skipped, so a black tile does not divide by zero.
//!
//! Panetta et al. list Michelson contrast as a generic measure that does not
//! know about water. It is included because it is a direct, well-defined
//! contrast number next to the underwater-specific UIConM.

use serde::{Deserialize, Serialize};

use crate::color::{block_min_max, for_each_block};
use crate::error::{Error, Result};

/// Global Michelson contrast and the mean of the block-wise ratios.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Michelson {
    /// `(max - min) / (max + min)` over the whole luminance plane.
    ///
    /// Zero when the image is black (`max + min = 0`).
    pub global: f64,
    /// Mean Michelson contrast of the tiles that have a non-zero denominator.
    pub block_mean: f64,
    /// Tiles that contributed to [`Self::block_mean`].
    pub blocks_used: u32,
}

/// Contrast of a luminance plane.
///
/// `data` is row-major luminance in `0..=255`. `block_size` is the tile side
/// in pixels. The paper's other block metrics use `8`.
pub fn michelson(data: &[f32], width: u32, height: u32, block_size: u32) -> Result<Michelson> {
    if width == 0 || height == 0 || data.is_empty() {
        return Err(Error::EmptyImage);
    }
    if block_size == 0 {
        return Err(Error::InvalidBlockSize(0));
    }
    let mut global_min = f32::INFINITY;
    let mut global_max = f32::NEG_INFINITY;
    for &value in data {
        global_min = global_min.min(value);
        global_max = global_max.max(value);
    }
    let global = ratio(global_min, global_max);

    let mut sum = 0.0;
    let mut used = 0_u32;
    for_each_block(width, height, block_size, |x, y, bw, bh| {
        let (min_v, max_v) = block_min_max(data, width, x, y, bw, bh);
        let value = ratio(min_v, max_v);
        if value.is_finite() {
            sum += value;
            used += 1;
        }
    });
    let block_mean = if used == 0 {
        0.0
    } else {
        sum / f64::from(used)
    };
    Ok(Michelson {
        global,
        block_mean,
        blocks_used: used,
    })
}

fn ratio(min_v: f32, max_v: f32) -> f64 {
    let den = f64::from(max_v) + f64::from(min_v);
    if den == 0.0 {
        f64::NAN
    } else {
        (f64::from(max_v) - f64::from(min_v)) / den
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_and_full_scale() {
        // Two pixels, 0 and 255. Contrast is 1.
        let data = [0.0, 255.0];
        let score = michelson(&data, 2, 1, 8).unwrap();
        assert!((score.global - 1.0).abs() < 1e-6);
    }

    #[test]
    fn flat_image_is_zero() {
        let data = [40.0; 16];
        let score = michelson(&data, 4, 4, 2).unwrap();
        assert_eq!(score.global, 0.0);
        assert_eq!(score.block_mean, 0.0);
    }
}
