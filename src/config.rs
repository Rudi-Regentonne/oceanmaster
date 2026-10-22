//! Knobs for which metrics run and how their formulas are parameterized.
//!
//! Paper defaults live on the `Default` impls. The command-line tool exposes
//! the ones people actually retune (UIQM weights, UICM alphas, UICM weights,
//! block size) and leaves the rest at the published values.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Which metrics [`crate::evaluate`] should compute.
///
/// Full-reference flags ([`Self::mse`], [`Self::rmse`], [`Self::mae`],
/// [`Self::psnr`]) require a reference image. Asking for them without one
/// returns [`Error::MissingReference`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricSet {
    /// Mean squared error.
    pub mse: bool,
    /// Root mean squared error.
    pub rmse: bool,
    /// Mean absolute error.
    pub mae: bool,
    /// Peak signal-to-noise ratio.
    pub psnr: bool,
    /// Global and block Michelson contrast.
    pub michelson: bool,
    /// Enhancement measure estimation.
    pub eme: bool,
    /// Weighted grayscale angle.
    pub gsa: bool,
    /// Analytical histogram statistics.
    pub histogram: bool,
    /// Underwater image colorfulness measure.
    pub uicm: bool,
    /// Underwater image sharpness measure.
    pub uism: bool,
    /// Underwater image contrast measure.
    pub uiconm: bool,
    /// Combined underwater image quality measure.
    ///
    /// Selecting this also computes UICM, UISM, and UIConM, because UIQM is
    /// their weighted sum.
    pub uiqm: bool,
}

impl MetricSet {
    /// No metrics selected.
    pub const fn none() -> Self {
        Self {
            mse: false,
            rmse: false,
            mae: false,
            psnr: false,
            michelson: false,
            eme: false,
            gsa: false,
            histogram: false,
            uicm: false,
            uism: false,
            uiconm: false,
            uiqm: false,
        }
    }

    /// Every metric, including full-reference ones.
    pub const fn all() -> Self {
        Self {
            mse: true,
            rmse: true,
            mae: true,
            psnr: true,
            michelson: true,
            eme: true,
            gsa: true,
            histogram: true,
            uicm: true,
            uism: true,
            uiconm: true,
            uiqm: true,
        }
    }

    /// Metrics that need only one image.
    pub const fn no_reference() -> Self {
        Self {
            michelson: true,
            eme: true,
            gsa: true,
            histogram: true,
            uicm: true,
            uism: true,
            uiconm: true,
            uiqm: true,
            ..Self::none()
        }
    }

    /// UICM, UISM, UIConM, and UIQM.
    pub const fn underwater() -> Self {
        Self {
            uicm: true,
            uism: true,
            uiconm: true,
            uiqm: true,
            ..Self::none()
        }
    }

    /// MSE, RMSE, MAE, and PSNR.
    pub const fn full_reference() -> Self {
        Self {
            mse: true,
            rmse: true,
            mae: true,
            psnr: true,
            ..Self::none()
        }
    }

    /// Union of two sets. A metric is kept if either side asked for it.
    pub const fn union(self, other: Self) -> Self {
        Self {
            mse: self.mse || other.mse,
            rmse: self.rmse || other.rmse,
            mae: self.mae || other.mae,
            psnr: self.psnr || other.psnr,
            michelson: self.michelson || other.michelson,
            eme: self.eme || other.eme,
            gsa: self.gsa || other.gsa,
            histogram: self.histogram || other.histogram,
            uicm: self.uicm || other.uicm,
            uism: self.uism || other.uism,
            uiconm: self.uiconm || other.uiconm,
            uiqm: self.uiqm || other.uiqm,
        }
    }

    /// Drop MSE, RMSE, MAE, and PSNR. Other flags stay as they are.
    pub const fn without_full_reference(self) -> Self {
        Self {
            mse: false,
            rmse: false,
            mae: false,
            psnr: false,
            ..self
        }
    }

    /// `true` when at least one metric is selected.
    pub const fn is_empty(self) -> bool {
        !self.mse
            && !self.rmse
            && !self.mae
            && !self.psnr
            && !self.michelson
            && !self.eme
            && !self.gsa
            && !self.histogram
            && !self.uicm
            && !self.uism
            && !self.uiconm
            && !self.uiqm
    }

    /// `true` when MSE, RMSE, MAE, or PSNR is selected.
    pub const fn any_full_reference(self) -> bool {
        self.mse || self.rmse || self.mae || self.psnr
    }

    /// `true` when UIQM or any of its three components is selected.
    pub const fn any_underwater(self) -> bool {
        self.uicm || self.uism || self.uiconm || self.uiqm
    }
}

impl Default for MetricSet {
    fn default() -> Self {
        Self::all()
    }
}

/// Trim ratios and the two regression weights of UICM equation (5).
///
/// There is no 6-coefficient vector in the paper. Colorfulness is
///
/// ```text
/// UICM = mean_weight * sqrt(μ_RG² + μ_YB²) + variance_weight * sqrt(σ_RG² + σ_YB²)
/// ```
///
/// with `mean_weight = -0.0268` and `variance_weight = 0.1586`.
/// `alpha_l` and `alpha_r` only decide how much of each sorted tail is
/// discarded before those means and variances are computed. The paper uses
/// `0.1` and `0.1` for the reported results.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UicmParams {
    /// Fraction of the lowest sorted opponent-channel samples to drop.
    /// Paper default: `0.1`.
    pub alpha_l: f64,
    /// Fraction of the highest sorted opponent-channel samples to drop.
    /// Paper default: `0.1`. Use a larger value for bubbles or light rays.
    pub alpha_r: f64,
    /// Weight on `sqrt(μ_RG² + μ_YB²)`. Paper default: `-0.0268`.
    ///
    /// Negative on purpose: a mean far from zero is a color cast.
    pub mean_weight: f64,
    /// Weight on `sqrt(σ_RG² + σ_YB²)`. Paper default: `0.1586`.
    ///
    /// Positive on purpose: more chroma variance is a wider color range.
    pub variance_weight: f64,
}

impl Default for UicmParams {
    fn default() -> Self {
        Self {
            alpha_l: 0.1,
            alpha_r: 0.1,
            mean_weight: -0.0268,
            variance_weight: 0.1586,
        }
    }
}

impl UicmParams {
    /// Reject ratios that would trim the whole distribution, and non-finite weights.
    pub fn validate(self) -> Result<()> {
        finite("uicm alpha_l", self.alpha_l)?;
        finite("uicm alpha_r", self.alpha_r)?;
        finite("uicm mean_weight", self.mean_weight)?;
        finite("uicm variance_weight", self.variance_weight)?;
        if !(0.0..1.0).contains(&self.alpha_l)
            || !(0.0..1.0).contains(&self.alpha_r)
            || self.alpha_l + self.alpha_r >= 1.0
        {
            return Err(Error::InvalidAlpha {
                alpha_l: self.alpha_l,
                alpha_r: self.alpha_r,
            });
        }
        Ok(())
    }
}

/// Channel weights and the edge floor used by UISM.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UismParams {
    /// Side length of the square EME blocks. The paper uses `8` for the
    /// numbers it reports.
    pub block_size: u32,
    /// Red-channel weight. Paper default: `0.299`.
    pub lambda_r: f64,
    /// Green-channel weight. Paper default: `0.587`.
    pub lambda_g: f64,
    /// Blue-channel weight. Paper default: `0.114`.
    pub lambda_b: f64,
    /// Sobel magnitude below this is not an edge, so it is left out of the
    /// block min/max. See [`crate::metrics::uism`] for why.
    pub edge_floor: f64,
}

impl Default for UismParams {
    fn default() -> Self {
        Self {
            block_size: 8,
            lambda_r: 0.299,
            lambda_g: 0.587,
            lambda_b: 0.114,
            edge_floor: 1.0,
        }
    }
}

impl UismParams {
    /// Reject a zero block size and non-finite weights.
    pub fn validate(self) -> Result<()> {
        if self.block_size == 0 {
            return Err(Error::InvalidBlockSize(0));
        }
        finite("uism lambda_r", self.lambda_r)?;
        finite("uism lambda_g", self.lambda_g)?;
        finite("uism lambda_b", self.lambda_b)?;
        finite("uism edge_floor", self.edge_floor)?;
        Ok(())
    }
}

/// Block size and PLIP gamma for UIConM.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiconmParams {
    /// Side length of the square blocks. The paper uses `8` for the numbers
    /// it reports.
    pub block_size: u32,
    /// Upper end of the PLIP gray scale. Must be greater than 255.
    ///
    /// `1026` is the usual 8-bit choice: every sample stays strictly below
    /// gamma, and `255 ⊕ 255` does not wrap.
    pub gamma: f64,
}

impl Default for UiconmParams {
    fn default() -> Self {
        Self {
            block_size: 8,
            gamma: 1026.0,
        }
    }
}

impl UiconmParams {
    /// Reject a zero block size or a gamma that cannot hold 8-bit samples.
    pub fn validate(self) -> Result<()> {
        if self.block_size == 0 {
            return Err(Error::InvalidBlockSize(0));
        }
        finite("uiconm gamma", self.gamma)?;
        if self.gamma <= 255.0 {
            return Err(Error::InvalidGamma(self.gamma));
        }
        Ok(())
    }
}

/// Linear combination weights of UIQM equation (10).
///
/// ```text
/// UIQM = c1 * UICM + c2 * UISM + c3 * UIConM
/// ```
///
/// The paper's generic set, fit by multiple linear regression on 30 images,
/// is `c1 = 0.0282`, `c2 = 0.2953`, `c3 = 3.5753`. Color-correction work
/// should raise `c1`. Visibility work should raise `c2` and `c3`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiqmParams {
    /// Weight on colorfulness. Paper default: `0.0282`.
    pub c1: f64,
    /// Weight on sharpness. Paper default: `0.2953`.
    pub c2: f64,
    /// Weight on contrast. Paper default: `3.5753`.
    pub c3: f64,
}

impl Default for UiqmParams {
    fn default() -> Self {
        Self {
            c1: 0.0282,
            c2: 0.2953,
            c3: 3.5753,
        }
    }
}

impl UiqmParams {
    /// Reject non-finite weights.
    pub fn validate(self) -> Result<()> {
        finite("uiqm c1", self.c1)?;
        finite("uiqm c2", self.c2)?;
        finite("uiqm c3", self.c3)?;
        Ok(())
    }
}

/// Parameters for the weighted grayscale-angle sharpness score.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GsaParams {
    /// Gradient magnitude, on the 0–255 luminance scale, that a pixel must
    /// reach before it counts as an edge. The detector also keeps only the
    /// strongest 10 percent, and uses whichever gate is higher.
    pub min_gradient: f64,
}

impl Default for GsaParams {
    fn default() -> Self {
        Self { min_gradient: 8.0 }
    }
}

impl GsaParams {
    /// Reject a non-finite threshold.
    pub fn validate(self) -> Result<()> {
        finite("gsa min_gradient", self.min_gradient)
    }
}

/// Everything [`crate::evaluate`] needs besides the pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Metrics to compute.
    pub metrics: MetricSet,
    /// Block side length shared by EME, block Michelson, UISM, and UIConM
    /// when those metrics are asked to follow the config rather than their
    /// own override. UISM and UIConM read their own `block_size`, which is
    /// set from this value by [`Config::with_block_size`].
    pub block_size: u32,
    /// UICM trim ratios and regression weights.
    pub uicm: UicmParams,
    /// UISM weights and edge floor.
    pub uism: UismParams,
    /// UIConM block size and PLIP gamma.
    pub uiconm: UiconmParams,
    /// UIQM combination weights.
    pub uiqm: UiqmParams,
    /// Grayscale-angle edge gate.
    pub gsa: GsaParams,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            metrics: MetricSet::all(),
            block_size: 8,
            uicm: UicmParams::default(),
            uism: UismParams::default(),
            uiconm: UiconmParams::default(),
            uiqm: UiqmParams::default(),
            gsa: GsaParams::default(),
        }
    }
}

impl Config {
    /// Set the shared block size and copy it into UISM and UIConM.
    ///
    /// The paper uses 8×8 blocks for the UISM and UIConM numbers it reports.
    /// Some public MATLAB ports use 5. Either value is accepted here.
    pub fn with_block_size(mut self, block_size: u32) -> Self {
        self.block_size = block_size;
        self.uism.block_size = block_size;
        self.uiconm.block_size = block_size;
        self
    }

    /// Check every parameter that can make a formula undefined.
    pub fn validate(&self) -> Result<()> {
        if self.block_size == 0 {
            return Err(Error::InvalidBlockSize(0));
        }
        self.uicm.validate()?;
        self.uism.validate()?;
        self.uiconm.validate()?;
        self.uiqm.validate()?;
        self.gsa.validate()?;
        Ok(())
    }
}

fn finite(name: &'static str, value: f64) -> Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(Error::NonFinite { name, value })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_uicm_defaults_match_equation_5() {
        let p = UicmParams::default();
        assert_eq!(p.alpha_l, 0.1);
        assert_eq!(p.alpha_r, 0.1);
        assert_eq!(p.mean_weight, -0.0268);
        assert_eq!(p.variance_weight, 0.1586);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn paper_uiqm_defaults() {
        let p = UiqmParams::default();
        assert_eq!(p.c1, 0.0282);
        assert_eq!(p.c2, 0.2953);
        assert_eq!(p.c3, 3.5753);
    }

    #[test]
    fn alpha_must_leave_a_pixel() {
        let p = UicmParams {
            alpha_l: 0.6,
            alpha_r: 0.6,
            ..UicmParams::default()
        };
        assert!(matches!(p.validate(), Err(Error::InvalidAlpha { .. })));
    }
}
