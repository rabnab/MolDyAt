// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! T/P field helpers for the renderer.
//!
//! NaN cells (underpopulated during binning) are filled by iterative
//! 4-connected neighbour averaging before the field is uploaded to the GPU,
//! which reconstructs it with a cubic B-spline filter.
//!
//! **Remaining-NaN behaviour**: 4-connected wavefront propagation can
//! require up to `(rows - 1) + (cols - 1)` passes to reach the cell at
//! maximum Manhattan distance from the nearest finite seed. The fill is
//! bounded by `rows + cols` passes. After that, either *all* cells are
//! finite (at least one input cell was finite) or *all* remain NaN
//! (whole grid was NaN).

/// Replace each NaN cell with the mean of its finite 4-connected neighbours.
///
/// Iterates up to `rows + cols` passes (sufficient for full Manhattan-distance
/// propagation), stopping early when no cell changes.  Used by the wgpu
/// renderer, which uploads the raw scalar values for GPU interpolation.
pub fn fill_nans_with_neighbour_mean(data: &[f32], rows: usize, cols: usize) -> Vec<f32> {
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
