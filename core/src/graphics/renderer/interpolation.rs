// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! T/P field interpolation for renderer upsampling.
//!
//! Upsamples sparse cell grids to screen-resolution pixel buffers using
//! `scirs2-interpolate`. NaN cells (underpopulated during binning) are
//! filled by iterative 4-connected neighbour averaging before the spline
//! is constructed.
//!
//! **Remaining-NaN behaviour**: 4-connected wavefront propagation can
//! require up to `(rows - 1) + (cols - 1)` passes to reach the cell at
//! maximum Manhattan distance from the nearest finite seed. The fill is
//! bounded by `rows + cols` passes. After that, either *all* cells are
//! finite (at least one input cell was finite) or *all* remain NaN
//! (whole grid was NaN). An all-NaN input produces an all-NaN output.

use crate::application::renderer_snapshot::ThermoGridSpec;
use crate::application::thermo_field::ThermoGrid;
use ndarray::{Array1, Array2};
use scirs2_interpolate::bivariate::{BivariateInterpolator, RectBivariateSpline};

/// Interpolate a sparse [`ThermoGrid`] to screen-resolution output.
///
/// 1. **NaN fill** — iterative 4-connected neighbour averaging (bounded
///    by `rows + cols` passes, sufficient for full Manhattan propagation).
/// 2. **Upsample** — bicubic `RectBivariateSpline` (falls back to lower
///    degree or nearest-neighbour when the grid is too small).
///
/// Returns a flat row-major `Vec<f32>` of length `target_w * target_h`.
/// An all-NaN input (or zero-size grid/target) returns all NaN.
pub fn interpolate_field(
    grid: &ThermoGrid,
    spec: &ThermoGridSpec,
    target_w: usize,
    target_h: usize,
) -> Vec<f32> {
    let (rows, cols) = grid.shape();
    let out_len = target_w * target_h;

    if rows == 0 || cols == 0 || target_w == 0 || target_h == 0 {
        return vec![f32::NAN; out_len];
    }

    // Step 1: NaN fill
    let filled = nan_fill(grid.as_slice(), rows, cols);

    // All-NaN → sentinel output
    if filled.iter().all(|v| !v.is_finite()) {
        return vec![f32::NAN; out_len];
    }

    // Step 2: Upsample
    upsample(&filled, rows, cols, spec, target_w, target_h)
}

/// Replace each NaN cell with the mean of its finite 4-connected neighbours.
///
/// Iterates up to `rows + cols` passes (sufficient for full Manhattan-distance
/// propagation), stopping early when no cell changes.  Exported for use by
/// the wgpu renderer which uploads raw scalar values for GPU interpolation.
pub fn fill_nans(data: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    nan_fill(data, rows, cols)
}

fn nan_fill(data: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    let mut src = data.to_vec();
    let mut dst = src.clone();
    let max_passes = rows + cols;

    for _ in 0..max_passes {
        let mut changed = false;

        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                if src[idx].is_finite() {
                    dst[idx] = src[idx];
                    continue;
                }
                let mut sum = 0.0f32;
                let mut count = 0u32;
                if r > 0 {
                    let v = src[(r - 1) * cols + c];
                    if v.is_finite() {
                        sum += v;
                        count += 1;
                    }
                }
                if r + 1 < rows {
                    let v = src[(r + 1) * cols + c];
                    if v.is_finite() {
                        sum += v;
                        count += 1;
                    }
                }
                if c > 0 {
                    let v = src[r * cols + c - 1];
                    if v.is_finite() {
                        sum += v;
                        count += 1;
                    }
                }
                if c + 1 < cols {
                    let v = src[r * cols + c + 1];
                    if v.is_finite() {
                        sum += v;
                        count += 1;
                    }
                }
                if count > 0 {
                    dst[idx] = sum / count as f32;
                    changed = true;
                } else {
                    dst[idx] = f32::NAN;
                }
            }
        }
        std::mem::swap(&mut src, &mut dst);
        if !changed {
            break;
        }
    }
    src
}

/// Upsample a fully-finite grid to `(target_h, target_w)` using spline
/// interpolation, falling back to nearest-neighbour for degenerate sizes.
fn upsample(
    filled: &[f32],
    rows: usize,
    cols: usize,
    spec: &ThermoGridSpec,
    target_w: usize,
    target_h: usize,
) -> Vec<f32> {
    // 1×1 grid — replicate the single value.
    if rows == 1 && cols == 1 {
        return vec![filled[0]; target_w * target_h];
    }

    // Need at least 2 points per dimension for bilinear.
    if rows < 2 || cols < 2 {
        return nearest_neighbor(filled, rows, cols, target_w, target_h);
    }

    // Cell-centre coordinates in world space.
    // RectBivariateSpline treats its first axis as rows and second as columns,
    // so row_coords maps rows→y (origin_y + cell_height) and col_coords maps
    // cols→x (origin_x + cell_width).
    let row_coords: Array1<f64> = Array1::from_vec(
        (0..rows)
            .map(|r| f64::from(spec.origin_y) + (r as f64 + 0.5) * f64::from(spec.cell_height))
            .collect(),
    );
    let col_coords: Array1<f64> = Array1::from_vec(
        (0..cols)
            .map(|c| f64::from(spec.origin_x) + (c as f64 + 0.5) * f64::from(spec.cell_width))
            .collect(),
    );

    // Data matrix (rows × cols).
    let z = Array2::from_shape_fn((rows, cols), |(r, c)| f64::from(filled[r * cols + c]));

    // Spline degree: bicubic when possible, otherwise max feasible.
    let kx = if rows >= 4 { 3 } else { (rows - 1).max(1) };
    let ky = if cols >= 4 { 3 } else { (cols - 1).max(1) };

    let spline = match RectBivariateSpline::new(
        &row_coords.view(),
        &col_coords.view(),
        &z.view(),
        None,
        kx,
        ky,
        None,
    ) {
        Ok(s) => s,
        Err(_) => return nearest_neighbor(filled, rows, cols, target_w, target_h),
    };

    // Target pixel-centre coordinates.
    let total_h = rows as f64 * f64::from(spec.cell_height);
    let total_w = cols as f64 * f64::from(spec.cell_width);

    let target_row_coords: Array1<f64> = Array1::from_vec(
        (0..target_h)
            .map(|r| f64::from(spec.origin_y) + (r as f64 + 0.5) / target_h as f64 * total_h)
            .collect(),
    );
    let target_col_coords: Array1<f64> = Array1::from_vec(
        (0..target_w)
            .map(|c| f64::from(spec.origin_x) + (c as f64 + 0.5) / target_w as f64 * total_w)
            .collect(),
    );

    // evaluate with grid=true → result shape (target_h, target_w).
    match spline.evaluate(&target_row_coords.view(), &target_col_coords.view(), true) {
        Ok(result) => {
            let mut out = Vec::with_capacity(target_w * target_h);
            for r in 0..target_h {
                for c in 0..target_w {
                    out.push(result[[r, c]] as f32);
                }
            }
            out
        }
        Err(_) => nearest_neighbor(filled, rows, cols, target_w, target_h),
    }
}

/// Simple nearest-neighbour upsampling fallback.
fn nearest_neighbor(
    data: &[f32],
    rows: usize,
    cols: usize,
    target_w: usize,
    target_h: usize,
) -> Vec<f32> {
    let mut out = Vec::with_capacity(target_w * target_h);
    for r in 0..target_h {
        let src_r = (r * rows / target_h).min(rows - 1);
        for c in 0..target_w {
            let src_c = (c * cols / target_w).min(cols - 1);
            out.push(data[src_r * cols + src_c]);
        }
    }
    out
}

/// Map a normalized value `t ∈ [0, 1]` to a rainbow colour.
///
/// The mapping is: blue (t = 0) → cyan → green → yellow → red (t = 1).
/// Alpha is fixed at 0.55.  Values outside `[0, 1]` are clamped.
pub fn rainbow_colormap(t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    // 4-segment piecewise linear: blue→cyan→green→yellow→red.
    let (r, g, b) = if t < 0.25 {
        // blue → cyan  (r=0, g rises, b=1)
        let s = t / 0.25;
        (0.0, s, 1.0)
    } else if t < 0.5 {
        // cyan → green  (r=0, g=1, b falls)
        let s = (t - 0.25) / 0.25;
        (0.0, 1.0, 1.0 - s)
    } else if t < 0.75 {
        // green → yellow  (r rises, g=1, b=0)
        let s = (t - 0.5) / 0.25;
        (s, 1.0, 0.0)
    } else {
        // yellow → red  (r=1, g falls, b=0)
        let s = (t - 0.75) / 0.25;
        (1.0, 1.0 - s, 0.0)
    };
    [r, g, b, 0.55]
}
