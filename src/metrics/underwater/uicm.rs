//! Underwater image colorfulness measure (UICM).
//!
//! Panetta, Gao, and Agaian, IEEE Journal of Oceanic Engineering, 2016,
//! section III-A. Color is measured in the opponent plane, not in RGB:
//!
//! ```text
//! RG = R - G                         (1)
//! YB = (R + G) / 2 - B               (2)
//! ```
//!
//! Underwater frames are noisy and often have a few very bright bubbles or
//! rays. The paper therefore does not use the ordinary mean. It sorts each
//! opponent channel and drops the tails. With `K` pixels,
//!
//! ```text
//! T_αL = ceil(α_L * K)     lowest samples discarded
//! T_αR = floor(α_R * K)    highest samples discarded
//! ```
//!
//! The asymmetric alpha-trimmed mean, equation (3), is the mean of what
//! remains. The second-order statistic, equation (4), is the population
//! variance of those same remaining samples. The prose says the tails are
//! trimmed for the variance as well as the mean; equation (4) is implemented
//! that way. The divisor is `K - T_αL - T_αR`, not `K - 1`.
//!
//! Equation (5) has two learned weights, not six:
//!
//! ```text
//! UICM = -0.0268 * sqrt(μ_RG² + μ_YB²) + 0.1586 * sqrt(σ_RG² + σ_YB²)
//! ```
//!
//! The first square root is how far the chroma sits from a neutral mean.
//! The weight is negative because a large mean is a cast. The second square
//! root is the chroma spread (the Euclidean norm of the two standard
//! deviations). The weight is positive because a wider spread is more color.
//! `α_L` and `α_R` are not weights. They only choose the trim. The paper
//! uses `0.1` and `0.1` for the results it prints. For a clean scene it
//! suggests `0`. For bubbles it suggests a larger `α_R`.
//!
//! Table I, Fig. 3(b) is reproduced by [`uicm_from_stats`] when the table's
//! means and standard deviations are plugged in.

use image::RgbImage;
use serde::{Deserialize, Serialize};

use crate::color::opponent;
use crate::config::UicmParams;
use crate::error::{Error, Result};

/// Trimmed opponent-channel statistics and the UICM score.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Uicm {
    /// Alpha-trimmed mean of `R - G`.
    pub mu_rg: f64,
    /// Alpha-trimmed mean of `(R + G) / 2 - B`.
    pub mu_yb: f64,
    /// Population standard deviation of the trimmed `RG` samples.
    ///
    /// This is the `σ_RG` column of Table I, not the variance.
    pub sigma_rg: f64,
    /// Population standard deviation of the trimmed `YB` samples.
    pub sigma_yb: f64,
    /// Population variance of the trimmed `RG` samples (`σ_RG²`).
    pub variance_rg: f64,
    /// Population variance of the trimmed `YB` samples (`σ_YB²`).
    pub variance_yb: f64,
    /// Equation (5).
    pub uicm: f64,
}

/// Colorfulness of an 8-bit RGB image.
pub fn uicm(image: &RgbImage, params: &UicmParams) -> Result<Uicm> {
    params.validate()?;
    if image.width() == 0 || image.height() == 0 {
        return Err(Error::EmptyImage);
    }
    let (mut rg, mut yb) = opponent(image);
    let (mu_rg, variance_rg) =
        alpha_trimmed_mean_variance(&mut rg, params.alpha_l, params.alpha_r)?;
    let (mu_yb, variance_yb) =
        alpha_trimmed_mean_variance(&mut yb, params.alpha_l, params.alpha_r)?;
    Ok(assemble(mu_rg, mu_yb, variance_rg, variance_yb, params))
}

/// Equation (5) from means and population variances that were already trimmed.
///
/// `variance_rg` and `variance_yb` are `σ²`, not standard deviations. Pass
/// `sigma * sigma` if you are copying a table that prints `σ`.
pub fn uicm_from_stats(
    mu_rg: f64,
    mu_yb: f64,
    variance_rg: f64,
    variance_yb: f64,
    params: &UicmParams,
) -> f64 {
    assemble(mu_rg, mu_yb, variance_rg, variance_yb, params).uicm
}

fn assemble(
    mu_rg: f64,
    mu_yb: f64,
    variance_rg: f64,
    variance_yb: f64,
    params: &UicmParams,
) -> Uicm {
    let mean_term = (mu_rg * mu_rg + mu_yb * mu_yb).sqrt();
    let spread_term = (variance_rg.max(0.0) + variance_yb.max(0.0)).sqrt();
    let score = params.mean_weight * mean_term + params.variance_weight * spread_term;
    Uicm {
        mu_rg,
        mu_yb,
        sigma_rg: variance_rg.max(0.0).sqrt(),
        sigma_yb: variance_yb.max(0.0).sqrt(),
        variance_rg,
        variance_yb,
        uicm: score,
    }
}

/// Mean and population variance after an asymmetric alpha trim.
///
/// `samples` is sorted in place. `alpha_l` and `alpha_r` are fractions in
/// `[0, 1)` whose sum is less than 1. The lowest `ceil(alpha_l * K)` and
/// highest `floor(alpha_r * K)` samples are discarded. Variance divides by
/// the number of samples that remain.
pub fn alpha_trimmed_mean_variance(
    samples: &mut [f32],
    alpha_l: f64,
    alpha_r: f64,
) -> Result<(f64, f64)> {
    let k = samples.len();
    if k == 0 {
        return Err(Error::EmptyImage);
    }
    let trim_l = (alpha_l * k as f64).ceil() as usize;
    let trim_r = (alpha_r * k as f64).floor() as usize;
    if trim_l.saturating_add(trim_r) >= k {
        return Err(Error::NotEnoughPixels {
            pixels: k,
            alpha_l,
            alpha_r,
        });
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let kept = &samples[trim_l..k - trim_r];
    let n = kept.len() as f64;
    let mut sum = 0.0;
    for &value in kept {
        sum += f64::from(value);
    }
    let mean = sum / n;
    let mut sum_sq = 0.0;
    for &value in kept {
        let delta = f64::from(value) - mean;
        sum_sq += delta * delta;
    }
    Ok((mean, sum_sq / n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn trim_matches_equation_3() {
        // K = 10, alpha = 0.1 → drop 1 from each end. Kept: 2..=9. Mean = 5.5.
        let mut samples: Vec<f32> = (1..=10).map(|v| v as f32).collect();
        let (mean, variance) = alpha_trimmed_mean_variance(&mut samples, 0.1, 0.1).unwrap();
        assert!((mean - 5.5).abs() < 1e-9);
        let expected_var = (2..=9)
            .map(|v| {
                let d = v as f64 - 5.5;
                d * d
            })
            .sum::<f64>()
            / 8.0;
        assert!((variance - expected_var).abs() < 1e-9);
    }

    #[test]
    fn table_i_fig_3b_reproduces_equation_5() {
        // Table I, Fig. 3(b): μ and σ, UICM = 4.9058.
        let params = UicmParams::default();
        let score = uicm_from_stats(
            0.7896,
            0.8901,
            23.9203_f64.powi(2),
            19.9269_f64.powi(2),
            &params,
        );
        assert!(
            (score - 4.9058).abs() < 5e-4,
            "equation (5) gave {score}, table says 4.9058"
        );
    }

    #[test]
    fn a_cast_with_no_spread_is_negative() {
        // Constant green-heavy pixel. Variance is 0, so only the negative
        // mean term survives.
        let image = RgbImage::from_pixel(8, 8, Rgb([20, 180, 40]));
        let score = uicm(&image, &UicmParams::default()).unwrap();
        assert!(score.variance_rg.abs() < 1e-9);
        assert!(score.uicm < 0.0);
        let rg = 20.0_f64 - 180.0;
        let yb = (20.0_f64 + 180.0) / 2.0 - 40.0;
        let expected = -0.0268 * (rg * rg + yb * yb).sqrt();
        assert!((score.uicm - expected).abs() < 1e-9);
    }

    #[test]
    fn zero_alpha_uses_every_pixel() {
        let mut samples = vec![1.0_f32, 3.0, 5.0];
        let (mean, _) = alpha_trimmed_mean_variance(&mut samples, 0.0, 0.0).unwrap();
        assert!((mean - 3.0).abs() < 1e-9);
    }
}
