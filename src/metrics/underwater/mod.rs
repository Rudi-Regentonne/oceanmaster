//! Underwater attribute measures from Panetta, Gao, and Agaian (2016).
//!
//! The paper's non-reference quality measure, UIQM, is the weighted sum of
//! three attributes that water actually damages:
//!
//! * [`uicm`] — colorfulness. Red disappears first, so frames go green or blue.
//! * [`uism`] — sharpness. Forward scatter blurs edges.
//! * [`uiconm`] — contrast. Backscatter lifts the blacks and flattens the scene.
//!
//! ```text
//! UIQM = c1 * UICM + c2 * UISM + c3 * UIConM
//! ```
//!
//! The generic weights are `0.0282`, `0.2953`, and `3.5753`. They are
//! overridable because the paper treats them as application-dependent.
//!
//! Citation: K. Panetta, C. Gao, and S. Agaian, "Human-Visual-System-Inspired
//! Underwater Image Quality Measures," IEEE Journal of Oceanic Engineering,
//! vol. 41, no. 3, pp. 541–551, 2016. DOI: 10.1109/JOE.2015.2469915.

mod uicm;
mod uiconm;
mod uiqm;
mod uism;

pub use uicm::{Uicm, alpha_trimmed_mean_variance, uicm, uicm_from_stats};
pub use uiconm::{log_amee, plip_add, plip_mul, plip_sub, uiconm};
pub use uiqm::{Uiqm, uiqm, uiqm_from_parts};
pub use uism::uism;
