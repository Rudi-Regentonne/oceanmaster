//! Weighted grayscale angle (WGSA).
//!
//! Hou and Weidemann proposed this as a no-reference sharpness score for
//! underwater images. Panetta et al. cite it as the measure based on the
//! weighted grayscale angles of detected edges. The original 2007 paper does
//! not print a line-by-line algorithm. The steps below follow the description
//! in that line of work and in US patent 8,437,568 (Hou, Weidemann): denoise,
//! find edges, measure the grayscale angle of each edge profile by linear
//! regression, then weight by the share of energy that lives in the
//! high-frequency wavelet band.
//!
//! What is computed:
//!
//! 1. Rec. 601 luminance is lightly denoised with a 3×3 box blur. This stands
//!    in for the patent's wavelet denoise and stops speckle from looking like
//!    a steep edge.
//! 2. Sobel `gx` and `gy` are taken on the denoised plane. A pixel is an edge
//!    when its magnitude is at least `min_gradient` and at least the 90th
//!    percentile of the magnitude image. The stricter of the two gates wins.
//! 3. At each edge pixel, five samples are taken along the gradient, two on
//!    each side. Ordinary least squares gives the slope of luminance versus
//!    distance in pixels. The grayscale angle is `atan(|slope|)` in degrees.
//!    A step edge is near 90°. A slow ramp is near 0°.
//! 4. A one-level orthonormal Haar transform of the original luminance supplies
//!    the weight: detail energy (`LH² + HL² + HH²`) divided by total energy.
//!    A blurry frame has little detail energy, so its weight is small.
//!
//! ```text
//! mean_angle = average grayscale angle of the edge pixels, in degrees
//! weight     = Haar detail energy / total energy
//! wgsa       = weight * mean_angle
//! ```
//!
//! Both the raw mean angle and the weighted score are returned. The weighted
//! score is the single number Hou and Weidemann use as the quality metric.
//! The mean angle is there so a sharp but low-contrast edge is not hidden by
//! the weight.

use serde::{Deserialize, Serialize};

use crate::color::{box_blur3, sample_bilinear, sobel};
use crate::config::GsaParams;
use crate::error::{Error, Result};

/// Edge-angle sharpness and the wavelet weight applied to it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GrayscaleAngle {
    /// Mean `atan(|slope|)` of the accepted edges, in degrees.
    ///
    /// Zero when no pixel passed the edge gate.
    pub mean_angle_deg: f64,
    /// Haar detail energy divided by total luminance energy, in `0..=1`.
    pub wavelet_weight: f64,
    /// `wavelet_weight * mean_angle_deg`. This is the weighted score.
    pub wgsa: f64,
    /// Pixels that contributed an angle.
    pub edge_count: u64,
}

/// Weighted grayscale angle of a luminance plane.
///
/// `data` is row-major Rec. 601 luminance. `params.min_gradient` is the
/// absolute magnitude floor on the 0–255 scale.
pub fn grayscale_angle(
    data: &[f32],
    width: u32,
    height: u32,
    params: &GsaParams,
) -> Result<GrayscaleAngle> {
    params.validate()?;
    if width == 0 || height == 0 || data.is_empty() {
        return Err(Error::EmptyImage);
    }
    let wavelet_weight = haar_detail_weight(data, width, height);
    let denoised = box_blur3(data, width, height);
    let gradient = sobel(&denoised, width, height);
    let gate = edge_gate(&gradient.magnitude, params.min_gradient);

    let mut angle_sum = 0.0;
    let mut edge_count = 0_u64;
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) as usize;
            let mag = gradient.magnitude[index];
            if mag < gate || mag == 0.0 {
                continue;
            }
            let dx = gradient.gx[index] / mag;
            let dy = gradient.gy[index] / mag;
            // Samples are centered, so Σ t = 0 and the slope is Σ(t*I) / Σ(t²).
            let mut sum_ti = 0.0;
            let mut sum_tt = 0.0;
            for t in -2..=2 {
                let tf = t as f32;
                let sx = x as f32 + tf * dx;
                let sy = y as f32 + tf * dy;
                let intensity = sample_bilinear(&denoised, width, height, sx, sy);
                sum_ti += f64::from(tf) * f64::from(intensity);
                sum_tt += f64::from(tf * tf);
            }
            if sum_tt == 0.0 {
                continue;
            }
            let slope = sum_ti / sum_tt;
            angle_sum += slope.abs().atan().to_degrees();
            edge_count += 1;
        }
    }
    let mean_angle_deg = if edge_count == 0 {
        0.0
    } else {
        angle_sum / edge_count as f64
    };
    Ok(GrayscaleAngle {
        mean_angle_deg,
        wavelet_weight,
        wgsa: wavelet_weight * mean_angle_deg,
        edge_count,
    })
}

fn edge_gate(magnitude: &[f32], floor: f64) -> f32 {
    if magnitude.is_empty() {
        return floor as f32;
    }
    let mut copy = magnitude.to_vec();
    copy.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((copy.len() as f64) * 0.90).floor() as usize;
    let index = index.min(copy.len() - 1);
    copy[index].max(floor as f32)
}

/// Detail-energy ratio of a one-level orthonormal Haar transform.
///
/// Odd trailing rows and columns are ignored so every coefficient comes from
/// a 2×2 block. A constant plane returns `0`.
pub fn haar_detail_weight(data: &[f32], width: u32, height: u32) -> f64 {
    let even_w = (width & !1) as usize;
    let even_h = (height & !1) as usize;
    if even_w < 2 || even_h < 2 {
        return 0.0;
    }
    let stride = width as usize;
    let mut detail = 0.0;
    let mut total = 0.0;
    for y in (0..even_h).step_by(2) {
        for x in (0..even_w).step_by(2) {
            let a = f64::from(data[y * stride + x]);
            let b = f64::from(data[y * stride + x + 1]);
            let c = f64::from(data[(y + 1) * stride + x]);
            let d = f64::from(data[(y + 1) * stride + x + 1]);
            let lh = 0.5 * (a - b + c - d);
            let hl = 0.5 * (a + b - c - d);
            let hh = 0.5 * (a - b - c + d);
            detail += lh * lh + hl * hl + hh * hh;
            total += a * a + b * b + c * c + d * d;
        }
    }
    if total == 0.0 { 0.0 } else { detail / total }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_plane_has_no_edges_and_no_detail() {
        let data = vec![40.0; 32 * 32];
        let score = grayscale_angle(&data, 32, 32, &GsaParams::default()).unwrap();
        assert_eq!(score.edge_count, 0);
        assert_eq!(score.mean_angle_deg, 0.0);
        assert_eq!(score.wavelet_weight, 0.0);
        assert_eq!(score.wgsa, 0.0);
    }

    #[test]
    fn a_step_is_steeper_than_a_ramp() {
        let step = vertical_edge(32, 32, 4);
        let ramp = vertical_ramp(32, 32);
        let params = GsaParams { min_gradient: 1.0 };
        let step_score = grayscale_angle(&step, 32, 32, &params).unwrap();
        let ramp_score = grayscale_angle(&ramp, 32, 32, &params).unwrap();
        assert!(step_score.edge_count > 0);
        assert!(ramp_score.edge_count > 0);
        assert!(step_score.mean_angle_deg > ramp_score.mean_angle_deg);
    }

    fn vertical_edge(width: usize, height: usize, span: usize) -> Vec<f32> {
        let mut data = vec![0.0; width * height];
        for y in 0..height {
            for x in 0..width {
                data[y * width + x] = if x < width / 2 { 20.0 } else { 220.0 };
            }
        }
        let _ = span;
        data
    }

    fn vertical_ramp(width: usize, height: usize) -> Vec<f32> {
        let mut data = vec![0.0; width * height];
        for y in 0..height {
            for x in 0..width {
                data[y * width + x] = (x as f32) * (200.0 / width as f32);
            }
        }
        data
    }
}
