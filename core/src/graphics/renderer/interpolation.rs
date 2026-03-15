//! T/P field interpolation for renderer upsampling.
//!
//! Upsamples sparse cell grids to screen-resolution pixel buffers using
//! `scirs2-interpolate`. NaN cells (underpopulated during binning) are
//! filled by iterative 4-connected neighbour averaging before the spline
//! is constructed.
//!
//! **Remaining-NaN behaviour**: In a rectangular grid every cell is
//! reachable from every other cell through 4-connectivity. Therefore,
//! after `max(rows, cols)` fill passes, either *all* cells are finite
//! (at least one input cell was finite) or *all* remain NaN (whole grid
//! was NaN). An all-NaN input produces an all-NaN output.

use crate::application::renderer_snapshot::ThermoGridSpec;
use crate::application::thermo_field::ThermoGrid;
use ndarray::{Array1, Array2};
use scirs2_interpolate::bivariate::{BivariateInterpolator, RectBivariateSpline};

/// Interpolate a sparse [`ThermoGrid`] to screen-resolution output.
///
/// 1. **NaN fill** — iterative 4-connected neighbour averaging (bounded
///    by `max(rows, cols)` passes).
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
/// Iterates up to `max(rows, cols)` passes, stopping early when no cell changes.
fn nan_fill(data: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    let mut buf = data.to_vec();
    let max_passes = rows.max(cols);

    for _ in 0..max_passes {
        let prev = buf.clone();
        let mut changed = false;

        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                if prev[idx].is_finite() {
                    continue;
                }
                let mut sum = 0.0f32;
                let mut count = 0u32;
                if r > 0 {
                    let v = prev[(r - 1) * cols + c];
                    if v.is_finite() {
                        sum += v;
                        count += 1;
                    }
                }
                if r + 1 < rows {
                    let v = prev[(r + 1) * cols + c];
                    if v.is_finite() {
                        sum += v;
                        count += 1;
                    }
                }
                if c > 0 {
                    let v = prev[r * cols + c - 1];
                    if v.is_finite() {
                        sum += v;
                        count += 1;
                    }
                }
                if c + 1 < cols {
                    let v = prev[r * cols + c + 1];
                    if v.is_finite() {
                        sum += v;
                        count += 1;
                    }
                }
                if count > 0 {
                    buf[idx] = sum / count as f32;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    buf
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
    let x_coords: Array1<f64> = Array1::from_vec(
        (0..rows)
            .map(|r| f64::from(spec.origin_y) + (r as f64 + 0.5) * f64::from(spec.cell_height))
            .collect(),
    );
    let y_coords: Array1<f64> = Array1::from_vec(
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
        &x_coords.view(),
        &y_coords.view(),
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

    let xi: Array1<f64> = Array1::from_vec(
        (0..target_h)
            .map(|r| f64::from(spec.origin_y) + (r as f64 + 0.5) / target_h as f64 * total_h)
            .collect(),
    );
    let yi: Array1<f64> = Array1::from_vec(
        (0..target_w)
            .map(|c| f64::from(spec.origin_x) + (c as f64 + 0.5) / target_w as f64 * total_w)
            .collect(),
    );

    // evaluate with grid=true → result shape (target_h, target_w).
    match spline.evaluate(&xi.view(), &yi.view(), true) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::thermo_field::ThermoGrid;

    fn make_spec(rows: usize, cols: usize) -> ThermoGridSpec {
        ThermoGridSpec {
            rows,
            cols,
            cell_width: 10.0,
            cell_height: 10.0,
            origin_x: 0.0,
            origin_y: 0.0,
        }
    }

    // ── Test 1: Uniform field ────────────────────────────────────────

    #[test]
    fn test_uniform_field() {
        let grid = ThermoGrid::filled(4, 4, 42.0);
        let spec = make_spec(4, 4);
        let out = interpolate_field(&grid, &spec, 20, 20);
        assert_eq!(out.len(), 400);
        for (i, &v) in out.iter().enumerate() {
            assert!((v - 42.0).abs() < 0.1, "pixel {i}: expected ~42.0, got {v}");
        }
    }

    // ── Test 2: Single NaN surrounded by finite cells ────────────────

    #[test]
    fn test_single_nan_filled() {
        let mut grid = ThermoGrid::filled(3, 3, 10.0);
        grid.set(1, 1, f32::NAN); // centre cell NaN
        let spec = make_spec(3, 3);
        let out = interpolate_field(&grid, &spec, 12, 12);
        assert_eq!(out.len(), 144);
        // The centre-ish pixel should be finite (NaN was filled).
        let mid = out[6 * 12 + 6];
        assert!(mid.is_finite(), "centre pixel should be finite, got {mid}");
    }

    // ── Test 3: Linear gradient preserved ────────────────────────────

    #[test]
    fn test_linear_gradient() {
        // Gradient increases left to right: col 0 = 0, col 1 = 10, etc.
        let rows = 4;
        let cols = 4;
        let mut grid = ThermoGrid::filled(rows, cols, 0.0);
        for r in 0..rows {
            for c in 0..cols {
                grid.set(r, c, c as f32 * 10.0);
            }
        }
        let spec = make_spec(rows, cols);
        let tw = 20;
        let th = 20;
        let out = interpolate_field(&grid, &spec, tw, th);
        assert_eq!(out.len(), tw * th);

        // Check gradient direction: rightmost column > leftmost column.
        let left = out[10 * tw]; // mid-row, first col
        let right = out[10 * tw + tw - 1]; // mid-row, last col
        assert!(
            right > left,
            "gradient should increase left→right: left={left}, right={right}"
        );

        // Corner values should approximately match input corners.
        // top-left input = 0, top-right input = 30.
        let tl = out[0];
        let tr = out[tw - 1];
        assert!(tl < 5.0, "top-left ~0, got {tl}");
        assert!(tr > 20.0, "top-right ~30, got {tr}");
    }

    // ── Test 4: All-NaN input ────────────────────────────────────────

    #[test]
    fn test_all_nan_input() {
        let grid = ThermoGrid::nan(3, 3);
        let spec = make_spec(3, 3);
        let out = interpolate_field(&grid, &spec, 10, 10);
        assert_eq!(out.len(), 100);
        assert!(
            out.iter().all(|v| v.is_nan()),
            "all-NaN input should produce all-NaN output"
        );
    }

    // ── Test 5: Output length correct ────────────────────────────────

    #[test]
    fn test_output_length() {
        let cases = [
            (2, 2, 8, 8),
            (3, 5, 20, 10),
            (4, 4, 100, 100),
            (1, 1, 16, 16),
            (6, 3, 30, 15),
        ];
        for (rows, cols, tw, th) in cases {
            let grid = ThermoGrid::filled(rows, cols, 1.0);
            let spec = make_spec(rows, cols);
            let out = interpolate_field(&grid, &spec, tw, th);
            assert_eq!(
                out.len(),
                tw * th,
                "rows={rows}, cols={cols}, tw={tw}, th={th}"
            );
        }
    }
}
