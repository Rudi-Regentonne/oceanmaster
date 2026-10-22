//! Full-reference error metrics.
//!
//! MSE, RMSE, MAE, and PSNR compare an image with a reference of the same
//! size. The error is taken over every RGB sample, so a color image with
//! `W×H` pixels contributes `3WH` terms. The peak value used by PSNR is
//! `255`, because images are evaluated as 8-bit RGB.
//!
//! ```text
//! MSE  = mean((a - b)²)
//! RMSE = sqrt(MSE)
//! MAE  = mean(|a - b|)
//! PSNR = 10 * log10(255² / MSE)
//! ```
//!
//! Identical images have `MSE = 0` and `PSNR = +∞`.

use image::RgbImage;

use crate::error::{Error, Result};

/// Peak sample value assumed by PSNR.
pub const PEAK: f64 = 255.0;

/// Squared error, absolute error, and the number of RGB samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrorSums {
    /// Sum of squared channel differences.
    pub sum_sq: f64,
    /// Sum of absolute channel differences.
    pub sum_abs: f64,
    /// `3 * width * height`.
    pub samples: u64,
}

/// Walk both images once and accumulate absolute and squared error.
pub fn error_sums(image: &RgbImage, reference: &RgbImage) -> Result<ErrorSums> {
    if image.width() != reference.width() || image.height() != reference.height() {
        return Err(Error::DimensionMismatch {
            left_width: image.width(),
            left_height: image.height(),
            right_width: reference.width(),
            right_height: reference.height(),
        });
    }
    if image.width() == 0 || image.height() == 0 {
        return Err(Error::EmptyImage);
    }
    let left = image.as_raw();
    let right = reference.as_raw();
    let mut sum_sq = 0.0;
    let mut sum_abs = 0.0;
    for (a, b) in left.iter().zip(right.iter()) {
        let diff = f64::from(*a) - f64::from(*b);
        sum_sq += diff * diff;
        sum_abs += diff.abs();
    }
    Ok(ErrorSums {
        sum_sq,
        sum_abs,
        samples: left.len() as u64,
    })
}

/// Mean squared error over every RGB sample.
pub fn mse(image: &RgbImage, reference: &RgbImage) -> Result<f64> {
    let sums = error_sums(image, reference)?;
    Ok(sums.sum_sq / sums.samples as f64)
}

/// Root mean squared error, `sqrt(MSE)`.
pub fn rmse(image: &RgbImage, reference: &RgbImage) -> Result<f64> {
    Ok(mse(image, reference)?.sqrt())
}

/// Mean absolute error over every RGB sample.
pub fn mae(image: &RgbImage, reference: &RgbImage) -> Result<f64> {
    let sums = error_sums(image, reference)?;
    Ok(sums.sum_abs / sums.samples as f64)
}

/// Peak signal-to-noise ratio in decibels.
///
/// Returns [`f64::INFINITY`] when the images are identical.
pub fn psnr(image: &RgbImage, reference: &RgbImage) -> Result<f64> {
    let value = mse(image, reference)?;
    Ok(psnr_from_mse(value))
}

/// PSNR from an already computed MSE. `0` becomes `+∞`.
pub fn psnr_from_mse(mse: f64) -> f64 {
    if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (PEAK * PEAK / mse).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn solid(value: u8) -> RgbImage {
        RgbImage::from_pixel(2, 2, Rgb([value, value, value]))
    }

    #[test]
    fn identical_images_have_zero_error_and_infinite_psnr() {
        let image = solid(10);
        assert_eq!(mse(&image, &image).unwrap(), 0.0);
        assert_eq!(rmse(&image, &image).unwrap(), 0.0);
        assert_eq!(mae(&image, &image).unwrap(), 0.0);
        assert!(psnr(&image, &image).unwrap().is_infinite());
    }

    #[test]
    fn known_difference() {
        let a = solid(0);
        let b = solid(2);
        // Every sample differs by 2. MSE = 4, RMSE = 2, MAE = 2.
        assert_eq!(mse(&a, &b).unwrap(), 4.0);
        assert_eq!(rmse(&a, &b).unwrap(), 2.0);
        assert_eq!(mae(&a, &b).unwrap(), 2.0);
        let expected = 10.0 * (255.0 * 255.0 / 4.0_f64).log10();
        assert!((psnr(&a, &b).unwrap() - expected).abs() < 1e-9);
    }

    #[test]
    fn size_mismatch_is_an_error() {
        let a = RgbImage::new(2, 2);
        let b = RgbImage::new(3, 2);
        assert!(matches!(mse(&a, &b), Err(Error::DimensionMismatch { .. })));
    }
}
