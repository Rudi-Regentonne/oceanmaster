//! Underwater image quality measure (UIQM).
//!
//! Panetta et al., section III-D, equation (10). Absorption and scattering
//! are treated as a linear mix, so the three attribute measures are mixed
//! the same way:
//!
//! ```text
//! UIQM = c1 * UICM + c2 * UISM + c3 * UIConM
//! ```
//!
//! The generic coefficient set, fit by multiple linear regression on 30
//! underwater images, is
//!
//! ```text
//! c1 = 0.0282    colorfulness
//! c2 = 0.2953    sharpness
//! c3 = 3.5753    contrast
//! ```
//!
//! The paper is explicit that these three numbers are application-dependent.
//! A color-correction comparison should put more weight on UICM. A visibility
//! comparison should put more weight on UISM and UIConM. Setting two of them
//! to zero makes UIQM fall back to a single attribute measure.
//!
//! A greater UIQM is a better image under the chosen weights. The paper
//! reports that a 10% rise is about the step a person can see.

use image::RgbImage;
use serde::{Deserialize, Serialize};

use crate::config::{UicmParams, UiconmParams, UiqmParams, UismParams};
use crate::error::Result;

use super::{Uicm, uicm, uiconm, uism};

/// The three attribute scores and their weighted sum.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Uiqm {
    /// Colorfulness, equation (5).
    pub uicm: Uicm,
    /// Sharpness, equations (6) and (7).
    pub uism: f64,
    /// Contrast, equations (8) and (9).
    pub uiconm: f64,
    /// `c1 * UICM + c2 * UISM + c3 * UIConM`.
    pub uiqm: f64,
}

/// Combined underwater quality of an 8-bit RGB image.
pub fn uiqm(
    image: &RgbImage,
    uicm_params: &UicmParams,
    uism_params: &UismParams,
    uiconm_params: &UiconmParams,
    weights: &UiqmParams,
) -> Result<Uiqm> {
    weights.validate()?;
    let color = uicm(image, uicm_params)?;
    let sharp = uism(image, uism_params)?;
    let contrast = uiconm(image, uiconm_params)?;
    Ok(uiqm_from_parts(color, sharp, contrast, weights))
}

/// Equation (10) from components that were already computed.
///
/// Useful when the caller already has UICM, UISM, and UIConM and only wants
/// to try a different weight set.
pub fn uiqm_from_parts(uicm: Uicm, uism: f64, uiconm: f64, weights: &UiqmParams) -> Uiqm {
    let score = weights.c1 * uicm.uicm + weights.c2 * uism + weights.c3 * uiconm;
    Uiqm {
        uicm,
        uism,
        uiconm,
        uiqm: score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{UicmParams, UiconmParams, UiqmParams, UismParams};
    use image::{Rgb, RgbImage};

    #[test]
    fn combination_is_the_weighted_sum() {
        let color = Uicm {
            mu_rg: 0.0,
            mu_yb: 0.0,
            sigma_rg: 0.0,
            sigma_yb: 0.0,
            variance_rg: 0.0,
            variance_yb: 0.0,
            uicm: 2.0,
        };
        let weights = UiqmParams::default();
        let score = uiqm_from_parts(color, 3.0, 4.0, &weights);
        let expected = 0.0282 * 2.0 + 0.2953 * 3.0 + 3.5753 * 4.0;
        assert!((score.uiqm - expected).abs() < 1e-9);
    }

    #[test]
    fn uiqm_runs_on_a_small_image() {
        // Edge sits inside the first 8×8 tile, and its brightness changes
        // down the frame so the edge-map max and min are not equal.
        let mut image = RgbImage::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let v = if x < 4 { 40 } else { 70 + (y as u8) * 8 };
                image.put_pixel(x, y, Rgb([v, v / 2, 90]));
            }
        }
        let score = uiqm(
            &image,
            &UicmParams::default(),
            &UismParams::default(),
            &UiconmParams::default(),
            &UiqmParams::default(),
        )
        .unwrap();
        assert!(score.uiqm.is_finite());
        assert!(
            score.uism > 0.0,
            "checker should have sharpness, got {}",
            score.uism
        );
    }
}
