//! Errors returned by loading, matching, and metric calculation.

use std::path::PathBuf;

/// Failure while loading an image or computing a metric.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The image decoder rejected the file.
    #[error("failed to read image {path}: {source}")]
    Image {
        /// Path that could not be decoded.
        path: PathBuf,
        /// Decoder error from the `image` crate.
        #[source]
        source: image::ImageError,
    },

    /// A directory listing or other filesystem call failed.
    #[error("failed to read {path}: {source}")]
    Io {
        /// Path that could not be read.
        path: PathBuf,
        /// Filesystem error.
        #[source]
        source: std::io::Error,
    },

    /// A report or other output file could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Destination path.
        path: PathBuf,
        /// Filesystem error.
        #[source]
        source: std::io::Error,
    },

    /// A reference folder has no file with this name.
    #[error("no reference named {name} in {directory}")]
    ReferenceNotFound {
        /// File name that was looked up.
        name: String,
        /// Folder that was searched.
        directory: PathBuf,
    },

    /// The path given as input does not exist.
    #[error("path does not exist: {0}")]
    NotFound(PathBuf),

    /// A directory contained no supported images.
    #[error("no supported images found in {0}")]
    NoImages(PathBuf),

    /// Folder comparison was asked for, but the reference is a single file.
    #[error(
        "input is a folder but the reference is a file; pass a reference folder so files can be matched by name"
    )]
    ReferenceIsFile,

    /// The image has no pixels.
    #[error("image is empty")]
    EmptyImage,

    /// Full-reference metrics need two images of equal size.
    #[error(
        "images have different sizes: {left_width}x{left_height} vs {right_width}x{right_height}"
    )]
    DimensionMismatch {
        /// Width of the image being evaluated.
        left_width: u32,
        /// Height of the image being evaluated.
        left_height: u32,
        /// Width of the reference.
        right_width: u32,
        /// Height of the reference.
        right_height: u32,
    },

    /// MSE, RMSE, MAE, or PSNR was requested without a reference.
    #[error("full-reference metrics (MSE, RMSE, MAE, PSNR) need a reference image")]
    MissingReference,

    /// `alpha_l` and `alpha_r` must leave at least one pixel.
    #[error(
        "UICM trim ratios must be in [0, 1) and sum to less than 1 (got alpha_l={alpha_l}, alpha_r={alpha_r})"
    )]
    InvalidAlpha {
        /// Left-tail trim ratio.
        alpha_l: f64,
        /// Right-tail trim ratio.
        alpha_r: f64,
    },

    /// Trimming removed every pixel. Happens on tiny images with a non-zero alpha.
    #[error("not enough pixels ({pixels}) to trim alpha_l={alpha_l} and alpha_r={alpha_r}")]
    NotEnoughPixels {
        /// Pixel count of the image.
        pixels: usize,
        /// Left-tail trim ratio.
        alpha_l: f64,
        /// Right-tail trim ratio.
        alpha_r: f64,
    },

    /// Block size must be a positive integer.
    #[error("block size must be at least 1, got {0}")]
    InvalidBlockSize(u32),

    /// PLIP gamma must sit strictly above every 8-bit sample.
    #[error("PLIP gamma must be greater than 255 so 8-bit samples stay inside the model, got {0}")]
    InvalidGamma(f64),

    /// A weight or threshold was NaN or infinite.
    #[error("{name} must be a finite number, got {value}")]
    NonFinite {
        /// Parameter name.
        name: &'static str,
        /// Rejected value.
        value: f64,
    },

    /// `--metrics` contained a name this crate does not know.
    #[error("unknown metric '{0}'. Try --help for the names and presets")]
    UnknownMetric(String),

    /// A caller passed a value this crate will not run with.
    #[error("{0}")]
    Invalid(String),
}

/// Convenient result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
