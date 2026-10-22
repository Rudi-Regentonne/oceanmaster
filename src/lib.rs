//! Underwater and general image-quality metrics.
//!
//! `oceanmaster` is both a library and a command-line tool. Link against this
//! crate when another program needs the numbers. Run the `oceanmaster` binary
//! when you have files or folders to score.
//!
//! # What it measures
//!
//! No-reference metrics look at one image:
//!
//! * **UICM** — underwater colorfulness. Opponent-channel mean and spread,
//!   with an asymmetric alpha trim. Two weights, not six. See [`metrics::uicm`].
//! * **UISM** — underwater sharpness. EME of a Sobel edge map on each channel.
//! * **UIConM** — underwater contrast. `logAMEE` of luminance, with PLIP
//!   arithmetic inside each block.
//! * **UIQM** — `c1*UICM + c2*UISM + c3*UIConM`. The paper's generic weights
//!   are `0.0282`, `0.2953`, and `3.5753`.
//! * **Michelson contrast** — `(max - min) / (max + min)`, globally and by block.
//! * **EME** — enhancement measure estimation on luminance.
//! * **Weighted grayscale angle** — Hou and Weidemann edge-profile sharpness,
//!   weighted by Haar detail energy.
//! * **Histogram** — mean, variance, skewness, excess kurtosis, dynamic range,
//!   and Shannon entropy, for luminance and for each RGB channel.
//!
//! Full-reference metrics need a second image of the same size:
//!
//! * **MSE**, **RMSE**, **MAE**, and **PSNR** over every RGB sample. PSNR uses
//!   a peak of 255. Identical images report `PSNR = +∞`.
//!
//! Images are decoded to 8-bit RGB. JPEG, PNG, WebP, TIFF, BMP, GIF (first
//! frame), and TGA are supported.
//!
//! # Library
//!
//! ```
//! use image::{Rgb, RgbImage};
//! use oceanmaster::{evaluate, Config, MetricSet};
//!
//! let image = RgbImage::from_pixel(16, 16, Rgb([40, 120, 90]));
//! let mut config = Config::default();
//! config.metrics = MetricSet::underwater();
//! let report = evaluate(&image, None, &config).unwrap();
//! let underwater = report.underwater.unwrap();
//! let _uiqm = underwater.uiqm.unwrap();
//! ```
//!
//! Single metrics are also public: [`mse`], [`psnr`], [`uicm`], [`uiqm`], and
//! the rest of [`metrics`].
//!
//! # Command-line tool
//!
//! ```text
//! oceanmaster -i reef.jpg
//! oceanmaster -i ./photos -o report.csv
//! oceanmaster -i enhanced.png -r original.png
//! oceanmaster -i enhanced.png --before original.png
//! oceanmaster -i ./enhanced --before ./original --metrics underwater
//! oceanmaster -i ./enhanced -r ./original --metrics underwater,psnr
//! ```
//!
//! `--input` is a file or a folder. `--reference` is the pixel-error partner
//! for MSE, RMSE, MAE, and PSNR. `--before` is the unprocessed image: each
//! no-reference score is printed for that image and for `--input`, plus the
//! change (`after - before`). In folder mode a partner is chosen by the same
//! relative path, then by the same file stem if the extension differs.
//! `--recursive` walks subfolders.
//!
//! `--metrics` accepts `auto` (the default), `all`, `underwater`, `noref`,
//! `reference`, or a comma-separated list such as `uicm,uiqm,psnr`. `auto`
//! runs the no-reference metrics, and adds MSE, RMSE, MAE, and PSNR when a
//! reference was given.
//!
//! Weights you are likely to change are flags, so `--help` stays short:
//!
//! * `--c1`, `--c2`, `--c3` — UIQM weights (defaults `0.0282`, `0.2953`, `3.5753`)
//! * `--uicm-alpha-l`, `--uicm-alpha-r` — tail trim (default `0.1` and `0.1`)
//! * `--uicm-mean-weight`, `--uicm-var-weight` — equation (5) (defaults `-0.0268`, `0.1586`)
//! * `--block-size` — EME, Michelson tiles, UISM, and UIConM (default `8`)
//!
//! `--format table` prints a readable table. `--format json` and `--format csv`
//! are the machine-readable forms. `--output` writes that same format to a file.
//!
//! Generate these docs with `cargo doc --no-deps --open`.
//!
//! # Papers
//!
//! Underwater formulas follow K. Panetta, C. Gao, and S. Agaian,
//! "Human-Visual-System-Inspired Underwater Image Quality Measures,"
//! *IEEE Journal of Oceanic Engineering*, vol. 41, no. 3, pp. 541–551, 2016.
//! The weighted grayscale angle follows the Hou and Weidemann edge-profile
//! method those authors cite. Where a printed equation is undefined on real
//! pixels (a zero minimum inside EME, a PLIP multiply of a negative sum), the
//! module docs say which defined reading is used.

#![warn(missing_docs)]

pub mod color;
pub mod compare;
pub mod config;
pub mod error;
pub mod input;
pub mod metrics;
pub mod report;

mod evaluate;

pub use compare::{Comparison, ScoreChange, compare_files, compare_scores};
pub use config::{Config, GsaParams, MetricSet, UicmParams, UiconmParams, UiqmParams, UismParams};
pub use error::{Error, Result};
pub use evaluate::{evaluate, evaluate_job};
pub use image::{Rgb, RgbImage};
pub use input::{IMAGE_EXTENSIONS, Job, discover, load};
pub use metrics::eme::eme;
pub use metrics::grayscale_angle;
pub use metrics::gsa::GrayscaleAngle;
pub use metrics::histogram::{ChannelStats, Histogram, histogram};
pub use metrics::michelson::{Michelson, michelson};
pub use metrics::underwater::{Uicm, Uiqm};
pub use metrics::{mae, mse, psnr, rmse, uicm, uiconm, uiqm, uism};
pub use report::{FullReference, Metrics, Report, Underwater};

/// Names and presets accepted by the `--metrics` flag.
///
/// `auto` depends on whether a reference will be available, so it is resolved
/// here rather than stored on [`MetricSet`]. `has_reference` should be true
/// when the caller passed a reference file or folder, even if a particular
/// image later fails to match.
pub fn parse_metrics(spec: &str, has_reference: bool) -> Result<MetricSet> {
    let mut set = MetricSet::none();
    let mut saw_auto = false;
    let mut saw_any = false;
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        saw_any = true;
        match part.to_ascii_lowercase().as_str() {
            "auto" => saw_auto = true,
            "all" => {
                set = set.union(if has_reference {
                    MetricSet::all()
                } else {
                    MetricSet::no_reference()
                });
            }
            "underwater" => set = set.union(MetricSet::underwater()),
            "noref" | "no-reference" | "nr" => set = set.union(MetricSet::no_reference()),
            "reference" | "full-reference" | "fullref" | "ref" | "fr" => {
                set = set.union(MetricSet::full_reference());
            }
            "mse" => set.mse = true,
            "rmse" => set.rmse = true,
            "mae" => set.mae = true,
            "psnr" => set.psnr = true,
            "michelson" | "michelson-contrast" => set.michelson = true,
            "eme" => set.eme = true,
            "gsa" | "wgsa" | "grayscale-angle" | "grayscale-angles" => set.gsa = true,
            "histogram" | "hist" => set.histogram = true,
            "uicm" => set.uicm = true,
            "uism" => set.uism = true,
            "uiconm" => set.uiconm = true,
            "uiqm" => set.uiqm = true,
            other => return Err(Error::UnknownMetric(other.to_string())),
        }
    }
    if !saw_any {
        return Err(Error::UnknownMetric("empty --metrics list".to_string()));
    }
    if saw_auto {
        set = set.union(if has_reference {
            MetricSet::all()
        } else {
            MetricSet::no_reference()
        });
    }
    if set.is_empty() {
        return Err(Error::UnknownMetric("no metrics selected".to_string()));
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_without_a_reference_skips_psnr() {
        let set = parse_metrics("auto", false).unwrap();
        assert!(set.uiqm);
        assert!(!set.psnr);
    }

    #[test]
    fn comma_list_can_mix_a_preset_and_a_name() {
        let set = parse_metrics("underwater,psnr", true).unwrap();
        assert!(set.uicm && set.uiqm && set.psnr);
        assert!(!set.mse);
    }
}
