//! Underwater image contrast measure (UIConM).
//!
//! Panetta et al., section III-C, equations (8) and (9). Contrast is
//! `logAMEE` of the intensity image, not a per-channel sum:
//!
//! ```text
//! UIConM = logAMEE(Intensity)
//! ```
//!
//! Intensity is Rec. 601 luminance, the same weights the paper uses for the
//! color channels of UISM (`0.299 R + 0.587 G + 0.114 B`).
//!
//! `logAMEE` is Agaian's measure of enhancement by entropy, written with the
//! parameterized logarithmic image-processing (PLIP) operators. Inside each
//! `k1 × k2` tile the Michelson ratio uses PLIP subtraction and addition.
//! The fraction bar and the product with the log are ordinary arithmetic,
//! as printed in equation (9):
//!
//! ```text
//! r = (Imax ⊖ Imin) / (Imax ⊕ Imin)
//! block term = r * log(r)
//! ```
//!
//! PLIP, with gamma `γ` (default `1026` for 8-bit images):
//!
//! ```text
//! a ⊕ b = a + b - a*b/γ
//! a ⊖ b = γ * (a - b) / (γ - b)
//! c ⊗ a = γ - γ * (1 - a/γ)^c
//! ```
//!
//! The outer `1/(k1*k2) ⊗ Σ ...` is the PLIP way of writing "scale the sum
//! by the tile count". For a sum that stays below `γ` it is almost the
//! arithmetic mean. On a real image the sum of the block terms exceeds `γ`,
//! and each term is negative because `r < 1`, so `log(r) < 0`. PLIP scalar
//! multiplication is only defined for a gray level in `[0, γ)`. The outer
//! operator is therefore evaluated as the absolute value of the arithmetic
//! mean of the block terms:
//!
//! ```text
//! UIConM = | mean(r * ln(r)) |
//! ```
//!
//! Undefined tiles (`Imax` too close to `γ`, a zero denominator, or
//! `r` outside `(0, 1)`) contribute nothing, and the divisor remains the
//! full tile count. `log` is the natural logarithm.
//!
//! `|r * ln(r)|` is the entropy of the tile ratio. It rises as contrast
//! leaves zero, peaks near `r = 1/e`, and falls again for a near-binary
//! tile. The paper's "greater UIConM means better contrast" describes the
//! usual underwater case, a flat frame being pulled back toward that peak,
//! not every possible increase in `(max - min)`.

use image::RgbImage;

use crate::color::{for_each_block, luminance};
use crate::config::UiconmParams;
use crate::error::Result;

/// Contrast of an 8-bit RGB image.
pub fn uiconm(image: &RgbImage, params: &UiconmParams) -> Result<f64> {
    params.validate()?;
    if image.width() == 0 || image.height() == 0 {
        return Err(crate::error::Error::EmptyImage);
    }
    let plane = luminance(image);
    log_amee(&plane, image.width(), image.height(), params)
}

/// `logAMEE` of a row-major intensity plane in `0..=255`.
pub fn log_amee(data: &[f32], width: u32, height: u32, params: &UiconmParams) -> Result<f64> {
    params.validate()?;
    if width == 0 || height == 0 || data.is_empty() {
        return Err(crate::error::Error::EmptyImage);
    }
    let (rows, cols) = for_each_block(width, height, params.block_size, |_, _, _, _| {});
    let tile_count = f64::from(rows) * f64::from(cols);
    if tile_count == 0.0 {
        return Ok(0.0);
    }
    let gamma = params.gamma;
    let mut sum = 0.0;
    for_each_block(width, height, params.block_size, |x, y, bw, bh| {
        if let Some(term) = block_term(data, width, x, y, bw, bh, gamma) {
            sum += term;
        }
    });
    Ok((sum / tile_count).abs())
}

fn block_term(
    data: &[f32],
    width: u32,
    x: u32,
    y: u32,
    bw: u32,
    bh: u32,
    gamma: f64,
) -> Option<f64> {
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for row in y..y + bh {
        let start = (row * width + x) as usize;
        for &value in &data[start..start + bw as usize] {
            min_v = min_v.min(value);
            max_v = max_v.max(value);
        }
    }
    let max_v = f64::from(max_v);
    let min_v = f64::from(min_v);
    if max_v <= min_v || max_v >= gamma || min_v >= gamma {
        return None;
    }
    let numerator = plip_sub(max_v, min_v, gamma)?;
    let denominator = plip_add(max_v, min_v, gamma);
    if denominator <= 0.0 {
        return None;
    }
    let ratio = numerator / denominator;
    if ratio <= 0.0 || ratio >= 1.0 {
        return None;
    }
    Some(ratio * ratio.ln())
}

/// PLIP addition: `a ⊕ b = a + b - a*b/γ`.
pub fn plip_add(a: f64, b: f64, gamma: f64) -> f64 {
    a + b - (a * b) / gamma
}

/// PLIP subtraction: `a ⊖ b = γ * (a - b) / (γ - b)`.
///
/// Returns `None` when `b` is not strictly below `γ`, where the model is
/// undefined.
pub fn plip_sub(a: f64, b: f64, gamma: f64) -> Option<f64> {
    let den = gamma - b;
    if den == 0.0 {
        None
    } else {
        Some(gamma * (a - b) / den)
    }
}

/// PLIP scalar multiplication: `c ⊗ a = γ - γ * (1 - a/γ)^c`.
///
/// Defined for a gray level `a` in `[0, γ)`. Exposed so the operator in
/// equation (9) can be inspected. [`uiconm`] does not call it for the outer
/// average; see the module docs for why.
pub fn plip_mul(c: f64, a: f64, gamma: f64) -> Option<f64> {
    if a < 0.0 || a >= gamma {
        return None;
    }
    Some(gamma - gamma * (1.0 - a / gamma).powf(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::UiconmParams;
    use image::{Rgb, RgbImage};

    #[test]
    fn plip_add_and_sub_match_the_definitions() {
        let gamma = 1026.0;
        assert!((plip_add(10.0, 20.0, gamma) - (30.0 - 200.0 / gamma)).abs() < 1e-9);
        let sub = plip_sub(30.0, 10.0, gamma).unwrap();
        assert!((sub - gamma * 20.0 / (gamma - 10.0)).abs() < 1e-9);
    }

    #[test]
    fn a_flat_image_has_no_contrast() {
        let image = RgbImage::from_pixel(16, 16, Rgb([40, 90, 110]));
        assert_eq!(uiconm(&image, &UiconmParams::default()).unwrap(), 0.0);
    }

    #[test]
    fn moderate_contrast_outranks_a_tiny_step() {
        // `|r * ln(r)|` rises with Michelson contrast until r = 1/e, then
        // falls again. A binary checker is past that peak and can score
        // *lower* than a moderate step. Stay on the rising side here.
        let mut tiny = RgbImage::new(8, 8);
        let mut moderate = RgbImage::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                let left = x < 4;
                tiny.put_pixel(
                    x,
                    y,
                    Rgb(if left {
                        [100, 100, 100]
                    } else {
                        [108, 108, 108]
                    }),
                );
                moderate.put_pixel(x, y, Rgb(if left { [80, 80, 80] } else { [160, 160, 160] }));
            }
        }
        let tiny_score = uiconm(&tiny, &UiconmParams::default()).unwrap();
        let moderate_score = uiconm(&moderate, &UiconmParams::default()).unwrap();
        assert!(tiny_score > 0.0, "tiny step scored {tiny_score}");
        assert!(
            moderate_score > tiny_score,
            "moderate {moderate_score} should beat tiny {tiny_score}"
        );
    }
}
