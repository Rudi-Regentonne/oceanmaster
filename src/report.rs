//! Results returned by [`crate::evaluate`].
//!
//! Every field that was not requested is `None`. JSON keeps the nested
//! structure. The command-line CSV writer flattens it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::metrics::gsa::GrayscaleAngle;
use crate::metrics::histogram::Histogram;
use crate::metrics::michelson::Michelson;
use crate::metrics::underwater::{Uicm, Uiqm};

/// One file, the metrics that were requested, and any non-fatal notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Image that was evaluated.
    pub path: PathBuf,
    /// Reference used for MSE, RMSE, MAE, and PSNR, if one was matched.
    pub reference: Option<PathBuf>,
    /// Width in pixels after conversion to 8-bit RGB.
    pub width: u32,
    /// Height in pixels after conversion to 8-bit RGB.
    pub height: u32,
    /// Notes that did not stop the evaluation. A missing folder match is the
    /// usual one.
    pub warnings: Vec<String>,
    /// Computed metrics. Unrequested families are `None`.
    pub metrics: Metrics,
}

/// Metrics for one image. Built either from files or from in-memory buffers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    /// Present when at least one full-reference metric was requested and a
    /// reference was supplied.
    pub full_reference: Option<FullReference>,
    /// Global and block Michelson contrast.
    pub michelson: Option<Michelson>,
    /// Enhancement measure estimation on luminance.
    pub eme: Option<f64>,
    /// Weighted grayscale angle.
    pub grayscale_angle: Option<GrayscaleAngle>,
    /// Analytical histogram statistics.
    pub histogram: Option<Histogram>,
    /// UICM, UISM, UIConM, and UIQM. A component that was not requested is
    /// `None`, except that asking for UIQM fills all three components.
    pub underwater: Option<Underwater>,
}

impl Metrics {
    /// No scores computed.
    pub const fn empty() -> Self {
        Self {
            full_reference: None,
            michelson: None,
            eme: None,
            grayscale_angle: None,
            histogram: None,
            underwater: None,
        }
    }
}

/// MSE, RMSE, MAE, and PSNR. A flag that was not requested stays `None`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FullReference {
    /// Mean squared error over every RGB sample.
    pub mse: Option<f64>,
    /// Root mean squared error.
    pub rmse: Option<f64>,
    /// Mean absolute error.
    pub mae: Option<f64>,
    /// Peak signal-to-noise ratio in decibels. Infinite when the images match.
    ///
    /// Serialized as the string `"inf"` rather than a non-standard JSON number.
    #[serde(serialize_with = "serialize_optional_psnr")]
    pub psnr: Option<f64>,
}

/// Underwater scores. `uiqm` is the weighted sum, not a fourth measurement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Underwater {
    /// Colorfulness and the trimmed opponent-channel statistics.
    pub uicm: Option<Uicm>,
    /// Sharpness.
    pub uism: Option<f64>,
    /// Contrast.
    pub uiconm: Option<f64>,
    /// Weighted sum. Present only when UIQM itself was requested.
    pub uiqm: Option<f64>,
}

impl Underwater {
    /// Fill the UIQM field from components that are already present.
    pub fn with_uiqm(mut self, combined: Uiqm) -> Self {
        self.uicm = Some(combined.uicm);
        self.uism = Some(combined.uism);
        self.uiconm = Some(combined.uiconm);
        self.uiqm = Some(combined.uiqm);
        self
    }
}

fn serialize_optional_psnr<S>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        None => serializer.serialize_none(),
        Some(number) if number.is_finite() => serializer.serialize_f64(*number),
        Some(number) if number.is_sign_positive() => serializer.serialize_str("inf"),
        Some(_) => serializer.serialize_str("-inf"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infinite_psnr_serializes_as_a_string() {
        let metrics = FullReference {
            mse: Some(0.0),
            rmse: Some(0.0),
            mae: Some(0.0),
            psnr: Some(f64::INFINITY),
        };
        let json = serde_json::to_string(&metrics).unwrap();
        assert!(json.contains("\"inf\""), "{json}");
    }
}
