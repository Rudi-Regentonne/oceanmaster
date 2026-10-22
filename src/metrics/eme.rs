//! Enhancement measure estimation (EME).
//!
//! This is equation (7) of Panetta et al. (2016), which is Agaian's
//! enhancement measure. The plane is split into `k1 × k2` tiles. Inside
//! each tile the score is the log contrast of the brightest and darkest
//! sample:
//!
//! ```text
//! EME = 2 / (k1 * k2) * Σ log(Imax / Imin)
//! ```
//!
//! `log` is the natural logarithm, matching the paper's mathematical `log`
//! and MATLAB's `log`. Tiles with `Imin <= 0` or `Imax <= Imin` have an
//! undefined ratio. They contribute nothing. The divisor stays `k1 * k2`,
//! the full tile count, as written in equation (7).
//!
//! The standalone EME metric runs on Rec. 601 luminance. UISM runs the same
//! function on each channel's grayscale edge map.

use crate::color::{block_min_max, for_each_block};
use crate::error::{Error, Result};

/// EME of a row-major plane.
///
/// Returns `0` when no tile has a defined ratio. That is a flat or empty
/// measurement, not an error.
pub fn eme(data: &[f32], width: u32, height: u32, block_size: u32) -> Result<f64> {
    if width == 0 || height == 0 || data.is_empty() {
        return Err(Error::EmptyImage);
    }
    if block_size == 0 {
        return Err(Error::InvalidBlockSize(0));
    }
    let (rows, cols) = for_each_block(width, height, block_size, |_, _, _, _| {});
    let tile_count = f64::from(rows) * f64::from(cols);
    if tile_count == 0.0 {
        return Ok(0.0);
    }
    let mut sum = 0.0;
    for_each_block(width, height, block_size, |x, y, bw, bh| {
        let (min_v, max_v) = block_min_max(data, width, x, y, bw, bh);
        if min_v > 0.0 && max_v > min_v {
            sum += (f64::from(max_v) / f64::from(min_v)).ln();
        }
    });
    Ok(2.0 * sum / tile_count)
}

/// EME when min and max are already restricted to a subset of the tile.
///
/// `defined` is called once per tile and returns `Some((min, max))` when
/// that tile has a usable pair. Undefined tiles contribute `0`.
pub fn eme_from_pairs(
    width: u32,
    height: u32,
    block_size: u32,
    mut defined: impl FnMut(u32, u32, u32, u32) -> Option<(f32, f32)>,
) -> Result<f64> {
    if width == 0 || height == 0 {
        return Err(Error::EmptyImage);
    }
    if block_size == 0 {
        return Err(Error::InvalidBlockSize(0));
    }
    let (rows, cols) = for_each_block(width, height, block_size, |_, _, _, _| {});
    let tile_count = f64::from(rows) * f64::from(cols);
    if tile_count == 0.0 {
        return Ok(0.0);
    }
    let mut sum = 0.0;
    for_each_block(width, height, block_size, |x, y, bw, bh| {
        if let Some((min_v, max_v)) = defined(x, y, bw, bh)
            && min_v > 0.0
            && max_v > min_v
        {
            sum += (f64::from(max_v) / f64::from(min_v)).ln();
        }
    });
    Ok(2.0 * sum / tile_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_tile_with_known_ratio() {
        // 2x2 tile, values 1 and e. log(e/1) = 1, times 2, one tile.
        let data = [1.0, 1.0, std::f32::consts::E, std::f32::consts::E];
        let score = eme(&data, 2, 2, 2).unwrap();
        assert!((score - 2.0).abs() < 1e-5);
    }

    #[test]
    fn flat_tile_contributes_nothing() {
        let data = [4.0; 4];
        assert_eq!(eme(&data, 2, 2, 2).unwrap(), 0.0);
    }
}
