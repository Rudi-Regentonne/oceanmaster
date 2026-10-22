//! Analytical histogram statistics.
//!
//! Panetta et al. note that a plain histogram is often used to judge an
//! underwater image: a picture people like usually occupies a wide range of
//! levels, and a cast shows up as one channel sitting far from the others.
//! This module does not just dump bins. It computes the distribution in
//! closed form from the samples:
//!
//! ```text
//! mean     = (1/N) Σ x
//! variance = (1/N) Σ (x - mean)²      population variance
//! std      = sqrt(variance)
//! skewness = (1/N) Σ ((x - mean) / std)³
//! kurtosis = (1/N) Σ ((x - mean) / std)⁴ - 3     excess kurtosis
//! entropy  = -Σ p_i log2(p_i)                    Shannon, 256 bins
//! range    = max - min
//! ```
//!
//! RGB moments use the raw 8-bit channels. Luminance moments use Rec. 601
//! luma. Luminance bins are the luma values rounded to the nearest integer
//! in `0..=255`, so entropy is comparable with the channel entropies.

use serde::{Deserialize, Serialize};

use crate::color::luminance_rgb;
use crate::error::{Error, Result};
use image::RgbImage;

/// Moments, entropy, and the 256-bin histogram of one plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelStats {
    /// Smallest sample.
    pub min: f64,
    /// Largest sample.
    pub max: f64,
    /// `max - min`. Zero on a flat plane.
    pub dynamic_range: f64,
    /// Arithmetic mean.
    pub mean: f64,
    /// Population variance, divided by `N` rather than `N - 1`.
    pub variance: f64,
    /// Square root of [`Self::variance`].
    pub std_dev: f64,
    /// Third standardized moment. Zero when the plane is flat.
    pub skewness: f64,
    /// Fourth standardized moment minus 3. Zero when the plane is flat.
    pub excess_kurtosis: f64,
    /// Shannon entropy of the 256-bin histogram, in bits.
    pub entropy_bits: f64,
    /// Bins whose count is greater than zero.
    pub used_bins: u32,
    /// Counts for levels `0..=255`. Always 256 entries.
    pub bins: Vec<u64>,
}

/// Luminance plus the three RGB channels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Histogram {
    /// Rec. 601 luminance.
    pub luminance: ChannelStats,
    /// Red channel.
    pub red: ChannelStats,
    /// Green channel.
    pub green: ChannelStats,
    /// Blue channel.
    pub blue: ChannelStats,
}

/// Histogram statistics of an 8-bit RGB image.
pub fn histogram(image: &RgbImage) -> Result<Histogram> {
    if image.width() == 0 || image.height() == 0 {
        return Err(Error::EmptyImage);
    }
    let raw = image.as_raw();
    let n = raw.len() / 3;
    if n == 0 {
        return Err(Error::EmptyImage);
    }

    let mut red = Accum::new();
    let mut green = Accum::new();
    let mut blue = Accum::new();
    let mut luma = Accum::new();
    for px in raw.as_chunks::<3>().0 {
        let r = f64::from(px[0]);
        let g = f64::from(px[1]);
        let b = f64::from(px[2]);
        red.push(r, px[0]);
        green.push(g, px[1]);
        blue.push(b, px[2]);
        let y = f64::from(luminance_rgb(px[0] as f32, px[1] as f32, px[2] as f32));
        let bin = y.round().clamp(0.0, 255.0) as u8;
        luma.push(y, bin);
    }

    Ok(Histogram {
        luminance: luma.finish(n),
        red: red.finish(n),
        green: green.finish(n),
        blue: blue.finish(n),
    })
}

struct Accum {
    min: f64,
    max: f64,
    sum: f64,
    sum_sq: f64,
    sum_cu: f64,
    sum_qu: f64,
    bins: Vec<u64>,
    started: bool,
}

impl Accum {
    fn new() -> Self {
        Self {
            min: 0.0,
            max: 0.0,
            sum: 0.0,
            sum_sq: 0.0,
            sum_cu: 0.0,
            sum_qu: 0.0,
            bins: vec![0; 256],
            started: false,
        }
    }
}

impl Accum {
    fn push(&mut self, value: f64, bin: u8) {
        if !self.started {
            self.min = value;
            self.max = value;
            self.started = true;
        } else {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
        self.sum += value;
        let sq = value * value;
        self.sum_sq += sq;
        self.sum_cu += sq * value;
        self.sum_qu += sq * sq;
        self.bins[bin as usize] += 1;
    }

    fn finish(self, n: usize) -> ChannelStats {
        let count = n as f64;
        let mean = self.sum / count;
        // Population central moments from raw power sums.
        let variance = (self.sum_sq / count) - mean * mean;
        let variance = variance.max(0.0);
        let std_dev = variance.sqrt();
        let (skewness, excess_kurtosis) = if std_dev == 0.0 {
            (0.0, 0.0)
        } else {
            let m3 =
                self.sum_cu / count - 3.0 * mean * self.sum_sq / count + 2.0 * mean * mean * mean;
            let m4 = self.sum_qu / count - 4.0 * mean * self.sum_cu / count
                + 6.0 * mean * mean * self.sum_sq / count
                - 3.0 * mean * mean * mean * mean;
            let skew = m3 / (std_dev * std_dev * std_dev);
            let kurt = m4 / (variance * variance) - 3.0;
            (skew, kurt)
        };
        let mut entropy = 0.0;
        let mut used = 0_u32;
        for &count_bin in &self.bins {
            if count_bin > 0 {
                used += 1;
                let p = count_bin as f64 / count;
                entropy -= p * p.log2();
            }
        }
        ChannelStats {
            min: self.min,
            max: self.max,
            dynamic_range: self.max - self.min,
            mean,
            variance,
            std_dev,
            skewness,
            excess_kurtosis,
            entropy_bits: entropy,
            used_bins: used,
            bins: self.bins,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn flat_channel_has_zero_entropy_and_range() {
        let image = RgbImage::from_pixel(4, 4, Rgb([10, 20, 30]));
        let stats = histogram(&image).unwrap();
        assert_eq!(stats.red.dynamic_range, 0.0);
        assert_eq!(stats.red.entropy_bits, 0.0);
        assert_eq!(stats.red.used_bins, 1);
        assert_eq!(stats.red.mean, 10.0);
        assert_eq!(stats.red.bins[10], 16);
        assert!((stats.green.mean - 20.0).abs() < 1e-9);
        assert!((stats.blue.mean - 30.0).abs() < 1e-9);
    }

    #[test]
    fn two_levels_have_one_bit_when_balanced() {
        let mut image = RgbImage::new(2, 1);
        image.put_pixel(0, 0, Rgb([0, 0, 0]));
        image.put_pixel(1, 0, Rgb([255, 0, 0]));
        let stats = histogram(&image).unwrap();
        assert!((stats.red.entropy_bits - 1.0).abs() < 1e-9);
        assert_eq!(stats.red.dynamic_range, 255.0);
        assert!(stats.red.skewness.abs() < 1e-9);
    }
}
