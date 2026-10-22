//! Image-quality metrics.
//!
//! Two families live here.
//!
//! Full-reference metrics need a second image of the same size. They answer
//! "how far is this from the reference?": [`mse`], [`rmse`], [`mae`], [`psnr`].
//!
//! No-reference metrics look at one image. The general ones are
//! [`michelson`], [`eme`], [`grayscale_angle`], and [`histogram`]. The
//! underwater ones are in [`underwater`] and follow Panetta et al. (2016).
//!
//! Most callers should use [`crate::evaluate`], which runs the selected set
//! and returns one report. The functions here are the direct versions, for
//! when only one number is needed.

pub mod eme;
pub mod full_ref;
pub mod gsa;
pub mod histogram;
pub mod michelson;
pub mod underwater;

pub use full_ref::{PEAK, mae, mse, psnr, psnr_from_mse, rmse};
pub use gsa::grayscale_angle;
pub use underwater::{uicm, uiconm, uiqm, uism};
