//! Before-and-after comparison of quality scores.
//!
//! Processing changes an image. Pixel error (MSE, PSNR) says how far the
//! pixels moved. This module says how the quality scores moved: each
//! no-reference number is computed on the unprocessed image and again on the
//! processed image, and the change is `after - before`.
//!
//! A positive change means the score went up. For UIQM and its parts, up is
//! the direction the paper treats as better. A score that can be negative,
//! such as UICM on a strong cast, can cross zero; the delta is still
//! `after - before`.

use std::path::{Path, PathBuf};

use image::RgbImage;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::evaluate::evaluate;
use crate::input::load;
use crate::report::{FullReference, Metrics};

/// One quality score on the unprocessed image, the processed image, and the change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreChange {
    /// Short name, the same one the CSV writer uses (`uiqm`, `uicm`, ...).
    pub name: String,
    /// Score of the unprocessed image.
    pub before: f64,
    /// Score of the processed image.
    pub after: f64,
    /// `after - before`.
    pub delta: f64,
}

/// Scores for one processed image and the unprocessed image it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// Processed image (`--input`).
    pub after: PathBuf,
    /// Unprocessed image (`--before`).
    pub before: PathBuf,
    /// Width of the processed image.
    pub width: u32,
    /// Height of the processed image.
    pub height: u32,
    /// Notes that did not stop the comparison. A size mismatch skips MSE and
    /// PSNR but still reports the quality-score changes.
    pub warnings: Vec<String>,
    /// No-reference scores present on both images, in a stable order.
    pub changes: Vec<ScoreChange>,
    /// Pixel error of the processed image against its reference, when those
    /// metrics were requested. The reference is `--reference` if it was
    /// passed, otherwise the `--before` image.
    pub full_reference: Option<FullReference>,
}

/// Pair every shared no-reference score.
///
/// A score that was not computed on both images is left out. Full-reference
/// numbers are not included here; they are not a property of one image.
pub fn compare_scores(before: &Metrics, after: &Metrics) -> Vec<ScoreChange> {
    let mut changes = Vec::new();
    push(
        &mut changes,
        "michelson",
        before.michelson.map(|score| score.global),
        after.michelson.map(|score| score.global),
    );
    push(
        &mut changes,
        "michelson_block",
        before.michelson.map(|score| score.block_mean),
        after.michelson.map(|score| score.block_mean),
    );
    push(&mut changes, "eme", before.eme, after.eme);
    push(
        &mut changes,
        "gsa_deg",
        before.grayscale_angle.map(|score| score.mean_angle_deg),
        after.grayscale_angle.map(|score| score.mean_angle_deg),
    );
    push(
        &mut changes,
        "gsa_weight",
        before.grayscale_angle.map(|score| score.wavelet_weight),
        after.grayscale_angle.map(|score| score.wavelet_weight),
    );
    push(
        &mut changes,
        "wgsa",
        before.grayscale_angle.map(|score| score.wgsa),
        after.grayscale_angle.map(|score| score.wgsa),
    );
    if let (Some(before_hist), Some(after_hist)) = (&before.histogram, &after.histogram) {
        push(
            &mut changes,
            "hist_entropy",
            Some(before_hist.luminance.entropy_bits),
            Some(after_hist.luminance.entropy_bits),
        );
        push(
            &mut changes,
            "hist_mean",
            Some(before_hist.luminance.mean),
            Some(after_hist.luminance.mean),
        );
        push(
            &mut changes,
            "hist_std",
            Some(before_hist.luminance.std_dev),
            Some(after_hist.luminance.std_dev),
        );
        push(
            &mut changes,
            "hist_range",
            Some(before_hist.luminance.dynamic_range),
            Some(after_hist.luminance.dynamic_range),
        );
        push(
            &mut changes,
            "hist_red_mean",
            Some(before_hist.red.mean),
            Some(after_hist.red.mean),
        );
        push(
            &mut changes,
            "hist_green_mean",
            Some(before_hist.green.mean),
            Some(after_hist.green.mean),
        );
        push(
            &mut changes,
            "hist_blue_mean",
            Some(before_hist.blue.mean),
            Some(after_hist.blue.mean),
        );
    }
    if let (Some(before_uw), Some(after_uw)) = (before.underwater, after.underwater) {
        push(
            &mut changes,
            "uicm",
            before_uw.uicm.map(|score| score.uicm),
            after_uw.uicm.map(|score| score.uicm),
        );
        push(
            &mut changes,
            "uicm_mu_rg",
            before_uw.uicm.map(|score| score.mu_rg),
            after_uw.uicm.map(|score| score.mu_rg),
        );
        push(
            &mut changes,
            "uicm_mu_yb",
            before_uw.uicm.map(|score| score.mu_yb),
            after_uw.uicm.map(|score| score.mu_yb),
        );
        push(
            &mut changes,
            "uicm_sigma_rg",
            before_uw.uicm.map(|score| score.sigma_rg),
            after_uw.uicm.map(|score| score.sigma_rg),
        );
        push(
            &mut changes,
            "uicm_sigma_yb",
            before_uw.uicm.map(|score| score.sigma_yb),
            after_uw.uicm.map(|score| score.sigma_yb),
        );
        push(&mut changes, "uism", before_uw.uism, after_uw.uism);
        push(&mut changes, "uiconm", before_uw.uiconm, after_uw.uiconm);
        push(&mut changes, "uiqm", before_uw.uiqm, after_uw.uiqm);
    }
    changes
}

fn push(out: &mut Vec<ScoreChange>, name: &'static str, before: Option<f64>, after: Option<f64>) {
    let (Some(before), Some(after)) = (before, after) else {
        return;
    };
    out.push(ScoreChange {
        name: name.to_string(),
        before,
        after,
        delta: after - before,
    });
}

/// Load a processed image and its unprocessed pair, then compare them.
///
/// `pixel_reference` is the image used for MSE, RMSE, MAE, and PSNR when
/// those metrics are selected. Pass `None` to use `before` for that role.
/// Quality-score changes do not need the two images to be the same size. A
/// size mismatch only skips the pixel-error numbers.
pub fn compare_files(
    after: &Path,
    before: &Path,
    pixel_reference: Option<&Path>,
    config: &Config,
) -> Result<Comparison> {
    config.validate()?;
    let after_image = load(after)?;
    let before_image = load(before)?;
    let loaded_reference = match pixel_reference {
        Some(path) if path != before => Some(load(path)?),
        _ => None,
    };
    let reference = loaded_reference.as_ref().unwrap_or(&before_image);
    compare_loaded(
        after,
        before,
        &after_image,
        &before_image,
        reference,
        config,
    )
}

fn compare_loaded(
    after: &Path,
    before: &Path,
    after_image: &RgbImage,
    before_image: &RgbImage,
    pixel_reference: &RgbImage,
    config: &Config,
) -> Result<Comparison> {
    let mut warnings = Vec::new();
    let mut score_config = config.clone();
    score_config.metrics = score_config.metrics.without_full_reference();
    let (before_scores, after_scores) = if score_config.metrics.is_empty() {
        (Metrics::empty(), Metrics::empty())
    } else {
        (
            evaluate(before_image, None, &score_config)?,
            evaluate(after_image, None, &score_config)?,
        )
    };

    let full_reference = if config.metrics.any_full_reference() {
        let mut error_config = config.clone();
        error_config.metrics = crate::config::MetricSet {
            michelson: false,
            eme: false,
            gsa: false,
            histogram: false,
            uicm: false,
            uism: false,
            uiconm: false,
            uiqm: false,
            ..config.metrics
        };
        match evaluate(after_image, Some(pixel_reference), &error_config) {
            Ok(metrics) => metrics.full_reference,
            Err(Error::DimensionMismatch { .. }) => {
                warnings.push(
                    "skipped MSE, RMSE, MAE, and PSNR because the images have different sizes"
                        .to_string(),
                );
                None
            }
            Err(err) => return Err(err),
        }
    } else {
        None
    };

    Ok(Comparison {
        after: after.to_path_buf(),
        before: before.to_path_buf(),
        width: after_image.width(),
        height: after_image.height(),
        warnings,
        changes: compare_scores(&before_scores, &after_scores),
        full_reference,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::underwater::Uicm;
    use crate::report::Underwater;

    fn uicm(value: f64) -> Uicm {
        Uicm {
            mu_rg: 0.0,
            mu_yb: 0.0,
            sigma_rg: 0.0,
            sigma_yb: 0.0,
            variance_rg: 0.0,
            variance_yb: 0.0,
            uicm: value,
        }
    }

    #[test]
    fn delta_is_after_minus_before() {
        let before = Metrics {
            underwater: Some(Underwater {
                uicm: Some(uicm(-1.0)),
                uism: Some(0.5),
                uiconm: None,
                uiqm: Some(1.0),
            }),
            ..Metrics::empty()
        };
        let after = Metrics {
            underwater: Some(Underwater {
                uicm: Some(uicm(0.5)),
                uism: Some(0.5),
                uiconm: None,
                uiqm: Some(2.5),
            }),
            ..Metrics::empty()
        };
        let changes = compare_scores(&before, &after);
        let uiqm = changes.iter().find(|change| change.name == "uiqm").unwrap();
        assert_eq!(uiqm.before, 1.0);
        assert_eq!(uiqm.after, 2.5);
        assert_eq!(uiqm.delta, 1.5);
        let uicm = changes.iter().find(|change| change.name == "uicm").unwrap();
        assert_eq!(uicm.delta, 1.5);
        assert!(changes.iter().all(|change| change.name != "uiconm"));
    }
}
