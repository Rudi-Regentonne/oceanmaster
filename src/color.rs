//! Pixel helpers shared by the metrics.
//!
//! Images are 8-bit RGB, as produced by [`crate::load`]. Luminance uses the
//! same Rec. 601 weights the paper uses for UISM (`0.299`, `0.587`, `0.114`).
//! Opponent channels are the ones in UICM equations (1) and (2):
//!
//! ```text
//! RG = R - G
//! YB = (R + G) / 2 - B
//! ```

use image::RgbImage;

/// Rec. 601 luma of one RGB pixel. Channels are `0..=255`.
#[inline]
pub fn luminance_rgb(r: f32, g: f32, b: f32) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// One luminance sample per pixel, row-major.
pub fn luminance(image: &RgbImage) -> Vec<f32> {
    let raw = image.as_raw();
    let mut out = Vec::with_capacity(raw.len() / 3);
    for px in raw.as_chunks::<3>().0 {
        out.push(luminance_rgb(px[0] as f32, px[1] as f32, px[2] as f32));
    }
    out
}

/// One color channel, `0` red, `1` green, `2` blue, as `f32` in `0..=255`.
pub fn channel(image: &RgbImage, index: usize) -> Vec<f32> {
    let raw = image.as_raw();
    let mut out = Vec::with_capacity(raw.len() / 3);
    for px in raw.as_chunks::<3>().0 {
        out.push(px[index] as f32);
    }
    out
}

/// UICM opponent channels, one pair of samples per pixel.
///
/// `rg` is `R - G`. `yb` is `(R + G) / 2 - B`. Both can be negative.
pub fn opponent(image: &RgbImage) -> (Vec<f32>, Vec<f32>) {
    let raw = image.as_raw();
    let n = raw.len() / 3;
    let mut rg = Vec::with_capacity(n);
    let mut yb = Vec::with_capacity(n);
    for px in raw.as_chunks::<3>().0 {
        let r = px[0] as f32;
        let g = px[1] as f32;
        let b = px[2] as f32;
        rg.push(r - g);
        yb.push((r + g) * 0.5 - b);
    }
    (rg, yb)
}

/// Sobel gradient of a row-major plane.
///
/// Borders are replicated. Magnitude is `sqrt(gx² + gy²)`, the ordinary
/// Sobel edge strength, not the sum of absolute filter responses.
pub struct Gradient {
    /// Horizontal derivative (vertical edges).
    pub gx: Vec<f32>,
    /// Vertical derivative (horizontal edges).
    pub gy: Vec<f32>,
    /// `sqrt(gx² + gy²)`.
    pub magnitude: Vec<f32>,
}

/// Compute [`Gradient`] for a single-channel plane.
pub fn sobel(data: &[f32], width: u32, height: u32) -> Gradient {
    let w = width as i32;
    let h = height as i32;
    let n = data.len();
    let mut gx = vec![0.0; n];
    let mut gy = vec![0.0; n];
    let mut magnitude = vec![0.0; n];
    if w == 0 || h == 0 {
        return Gradient { gx, gy, magnitude };
    }
    for y in 0..h {
        for x in 0..w {
            let gxv = -at(data, w, h, x - 1, y - 1) + at(data, w, h, x + 1, y - 1)
                - 2.0 * at(data, w, h, x - 1, y)
                + 2.0 * at(data, w, h, x + 1, y)
                - at(data, w, h, x - 1, y + 1)
                + at(data, w, h, x + 1, y + 1);
            let gyv = -at(data, w, h, x - 1, y - 1)
                - 2.0 * at(data, w, h, x, y - 1)
                - at(data, w, h, x + 1, y - 1)
                + at(data, w, h, x - 1, y + 1)
                + 2.0 * at(data, w, h, x, y + 1)
                + at(data, w, h, x + 1, y + 1);
            let i = (y as u32 * width + x as u32) as usize;
            gx[i] = gxv;
            gy[i] = gyv;
            magnitude[i] = (gxv * gxv + gyv * gyv).sqrt();
        }
    }
    Gradient { gx, gy, magnitude }
}

#[inline]
fn at(data: &[f32], width: i32, height: i32, x: i32, y: i32) -> f32 {
    let x = x.clamp(0, width - 1) as usize;
    let y = y.clamp(0, height - 1) as usize;
    data[y * width as usize + x]
}

/// 3×3 box blur with replicated borders. Used as a light denoise before
/// grayscale-angle edges are measured.
pub fn box_blur3(data: &[f32], width: u32, height: u32) -> Vec<f32> {
    let w = width as i32;
    let h = height as i32;
    let mut out = vec![0.0; data.len()];
    if w == 0 || h == 0 {
        return out;
    }
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    sum += at(data, w, h, x + dx, y + dy);
                }
            }
            out[(y as u32 * width + x as u32) as usize] = sum / 9.0;
        }
    }
    out
}

/// Bilinear sample with coordinates clamped to the plane.
pub fn sample_bilinear(data: &[f32], width: u32, height: u32, x: f32, y: f32) -> f32 {
    if width == 0 || height == 0 {
        return 0.0;
    }
    let max_x = (width - 1) as f32;
    let max_y = (height - 1) as f32;
    let x = x.clamp(0.0, max_x);
    let y = y.clamp(0.0, max_y);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let i00 = (y0 * width + x0) as usize;
    let i10 = (y0 * width + x1) as usize;
    let i01 = (y1 * width + x0) as usize;
    let i11 = (y1 * width + x1) as usize;
    let top = data[i00] * (1.0 - tx) + data[i10] * tx;
    let bot = data[i01] * (1.0 - tx) + data[i11] * tx;
    top * (1.0 - ty) + bot * ty
}

/// Visit every complete `block × block` tile.
///
/// A leftover strip thinner than `block` is left out, so the metric is not
/// changed by a resize. If the image is smaller than one block, the whole
/// image is visited once.
///
/// The callback receives `(x, y, tile_width, tile_height)`. The returned
/// pair is `(rows, columns)` of tiles, which the paper calls `k1` and `k2`.
pub fn for_each_block(
    width: u32,
    height: u32,
    block: u32,
    mut visit: impl FnMut(u32, u32, u32, u32),
) -> (u32, u32) {
    let block = block.max(1);
    if width == 0 || height == 0 {
        return (0, 0);
    }
    let cols = width / block;
    let rows = height / block;
    if rows == 0 || cols == 0 {
        visit(0, 0, width, height);
        return (1, 1);
    }
    for row in 0..rows {
        for col in 0..cols {
            visit(col * block, row * block, block, block);
        }
    }
    (rows, cols)
}

/// Min and max inside a tile. The plane is row-major with the given width.
pub fn block_min_max(data: &[f32], width: u32, x: u32, y: u32, bw: u32, bh: u32) -> (f32, f32) {
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for row in y..y + bh {
        let start = (row * width + x) as usize;
        for &value in &data[start..start + bw as usize] {
            min_v = min_v.min(value);
            max_v = max_v.max(value);
        }
    }
    (min_v, max_v)
}
