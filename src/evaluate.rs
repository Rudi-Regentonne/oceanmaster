//! Run a [`Config`] against one image or a matched pair.

use image::RgbImage;

use crate::color::luminance;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::input::{Job, load};
use crate::metrics::eme;
use crate::metrics::full_ref::{error_sums, psnr_from_mse};
use crate::metrics::gsa::grayscale_angle;
use crate::metrics::histogram::histogram;
use crate::metrics::michelson::michelson;
use crate::metrics::underwater::{uicm, uiconm, uiqm_from_parts, uism};
use crate::report::{FullReference, Metrics, Report, Underwater};

/// Evaluate `image`, optionally against `reference`.
///
/// Full-reference flags in `config.metrics` require `reference` and the two
/// images must have the same width and height. No-reference metrics ignore
/// `reference`. UIQM, if requested, computes UICM, UISM, and UIConM with the
/// parameters on `config` and then applies `config.uiqm`.
///
/// # Examples
///
/// ```
/// use image::{Rgb, RgbImage};
/// use oceanmaster::{evaluate, Config, MetricSet};
///
/// let image = RgbImage::from_pixel(16, 16, Rgb([20, 80, 90]));
/// let mut config = Config::default();
/// config.metrics = MetricSet::underwater();
/// let metrics = evaluate(&image, None, &config).unwrap();
/// assert!(metrics.underwater.unwrap().uiqm.unwrap().is_finite());
/// ```
pub fn evaluate(
    image: &RgbImage,
    reference: Option<&RgbImage>,
    config: &Config,
) -> Result<Metrics> {
    config.validate()?;
    if image.width() == 0 || image.height() == 0 {
        return Err(Error::EmptyImage);
    }
    let set = config.metrics;
    if set.any_full_reference() && reference.is_none() {
        return Err(Error::MissingReference);
    }

    let full_reference = if set.any_full_reference() {
        Some(full_reference(image, reference.unwrap(), config)?)
    } else {
        None
    };

    let needs_luma = set.michelson || set.eme || set.gsa;
    let luma = if needs_luma {
        Some(luminance(image))
    } else {
        None
    };
    let width = image.width();
    let height = image.height();

    let michelson = if set.michelson {
        Some(michelson(
            luma.as_ref().unwrap(),
            width,
            height,
            config.block_size,
        )?)
    } else {
        None
    };
    let eme = if set.eme {
        Some(eme::eme(
            luma.as_ref().unwrap(),
            width,
            height,
            config.block_size,
        )?)
    } else {
        None
    };
    let grayscale_angle = if set.gsa {
        Some(grayscale_angle(
            luma.as_ref().unwrap(),
            width,
            height,
            &config.gsa,
        )?)
    } else {
        None
    };
    let histogram = if set.histogram {
        Some(histogram(image)?)
    } else {
        None
    };
    let underwater = if set.any_underwater() {
        Some(underwater(image, config)?)
    } else {
        None
    };

    Ok(Metrics {
        full_reference,
        michelson,
        eme,
        grayscale_angle,
        histogram,
        underwater,
    })
}

/// Load a [`Job`], evaluate it, and attach the file paths to a [`Report`].
///
/// A job with no matched reference still produces no-reference metrics. If
/// the configuration asked only for full-reference metrics, that job returns
/// [`Error::MissingReference`].
pub fn evaluate_job(job: &Job, config: &Config) -> Result<Report> {
    let image = load(&job.input)?;
    let reference = match &job.reference {
        Some(path) => Some(load(path)?),
        None => None,
    };
    let mut config = config.clone();
    let mut warnings = Vec::new();
    if let Some(warning) = &job.warning {
        warnings.push(warning.clone());
    }
    if reference.is_none() && config.metrics.any_full_reference() {
        if job.warning.is_none() {
            warnings.push("no reference image; skipped MSE, RMSE, MAE, and PSNR".to_string());
        }
        config.metrics = config.metrics.without_full_reference();
        if config.metrics.is_empty() {
            return Err(Error::MissingReference);
        }
    }
    let metrics = evaluate(&image, reference.as_ref(), &config)?;
    Ok(Report {
        path: job.input.clone(),
        reference: job.reference.clone(),
        width: image.width(),
        height: image.height(),
        warnings,
        metrics,
    })
}

fn full_reference(
    image: &RgbImage,
    reference: &RgbImage,
    config: &Config,
) -> Result<FullReference> {
    let sums = error_sums(image, reference)?;
    let samples = sums.samples as f64;
    let mse = sums.sum_sq / samples;
    let set = config.metrics;
    Ok(FullReference {
        mse: set.mse.then_some(mse),
        rmse: set.rmse.then_some(mse.sqrt()),
        mae: set.mae.then_some(sums.sum_abs / samples),
        psnr: set.psnr.then_some(psnr_from_mse(mse)),
    })
}

fn underwater(image: &RgbImage, config: &Config) -> Result<Underwater> {
    let set = config.metrics;
    if set.uiqm {
        let color = uicm(image, &config.uicm)?;
        let sharp = uism(image, &config.uism)?;
        let contrast = uiconm(image, &config.uiconm)?;
        let combined = uiqm_from_parts(color, sharp, contrast, &config.uiqm);
        return Ok(Underwater {
            uicm: Some(combined.uicm),
            uism: Some(combined.uism),
            uiconm: Some(combined.uiconm),
            uiqm: Some(combined.uiqm),
        });
    }
    Ok(Underwater {
        uicm: if set.uicm {
            Some(uicm(image, &config.uicm)?)
        } else {
            None
        },
        uism: if set.uism {
            Some(uism(image, &config.uism)?)
        } else {
            None
        },
        uiconm: if set.uiconm {
            Some(uiconm(image, &config.uiconm)?)
        } else {
            None
        },
        uiqm: None,
    })
}
