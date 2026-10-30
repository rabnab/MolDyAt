// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::ball_impl::{Ball2D, ForceResult};
use crate::objects2d::{Rect, Vec2D};
use std::collections::VecDeque;

/// Center-of-mass-subtracted kinetic statistics for a local region.
#[derive(Clone, Debug)]
pub(crate) struct LocalKineticStats {
    count: usize,
    sum_m: f32,
    sum_mv: Vec2D,
    sum_mv2: f32,
}

impl LocalKineticStats {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Add one ball to the local kinetic accumulator.
    pub(crate) fn add_ball(&mut self, ball: &Ball2D) {
        let mass = ball.mass;
        self.count += 1;
        self.sum_m += mass;
        self.sum_mv.x += mass * ball.v.x;
        self.sum_mv.y += mass * ball.v.y;
        self.sum_mv2 += mass * ball.v.norm_sq();
    }

    pub(crate) fn count(&self) -> usize {
        self.count
    }

    /// Center-of-mass velocity for the accumulated region.
    ///
    /// The thermostat measures temperature in the local COM frame, so callers
    /// that later modify velocities should use this velocity to isolate the
    /// peculiar component `v - v_com` instead of damping the full drift motion.
    pub(crate) fn center_of_mass_velocity(&self) -> Option<Vec2D> {
        if self.count < 2 || self.sum_m <= f32::EPSILON {
            return None;
        }

        Some(self.sum_mv.scaled(1.0 / self.sum_m))
    }

    /// Local temperature in the center-of-mass frame.
    pub(crate) fn temperature(&self) -> Option<f32> {
        if self.count < 2 || self.sum_m <= f32::EPSILON {
            return None;
        }

        // Removing the 2D center-of-mass motion leaves 2 * (N - 1) thermal
        // degrees of freedom. The numerator is `sum m |v - v_com|^2`, i.e.
        // twice the thermal kinetic energy in reduced LJ units.
        let thermal_dof = 2.0 * (self.count as f32 - 1.0);
        if thermal_dof <= f32::EPSILON {
            return None;
        }

        // COM subtraction can leave a tiny negative/positive residue from
        // floating-point cancellation for rigidly drifting buckets.
        let thermal_speed_sq = (self.sum_mv2 - self.sum_mv.norm_sq() / self.sum_m).max(0.0);
        Some(thermal_speed_sq / thermal_dof)
    }
}

impl Default for LocalKineticStats {
    fn default() -> Self {
        Self {
            count: 0,
            sum_m: 0.0,
            sum_mv: Vec2D::zero(),
            sum_mv2: 0.0,
        }
    }
}

/// A flat, row-major grid of `f32` values with explicit dimensions.
///
/// Single contiguous allocation for cache-friendly iteration. Used for
/// per-cell temperature and pressure fields.
///
/// **Invariant**: `data.len() == rows * cols` (enforced by constructors).
#[derive(Clone, Debug)]
pub struct ThermoGrid {
    data: Vec<f32>,
    rows: usize,
    cols: usize,
}

impl ThermoGrid {
    /// Create a grid filled with the given value.
    pub fn filled(rows: usize, cols: usize, value: f32) -> Self {
        Self {
            data: vec![value; rows * cols],
            rows,
            cols,
        }
    }

    /// Read the value at `(r, c)`. Panics on out-of-bounds (standard Rust indexing).
    #[inline]
    pub fn get(&self, r: usize, c: usize) -> f32 {
        self.data[r * self.cols + c]
    }

    /// Write the value at `(r, c)`. Panics on out-of-bounds (standard Rust indexing).
    #[inline]
    pub fn set(&mut self, r: usize, c: usize, val: f32) {
        self.data[r * self.cols + c] = val;
    }

    /// Returns `(rows, cols)`.
    pub fn shape(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    #[cfg(test)]
    pub(crate) fn rows(&self) -> usize {
        self.rows
    }

    #[cfg(test)]
    pub(crate) fn cols(&self) -> usize {
        self.cols
    }

    /// Borrow the underlying flat data as a slice.
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }
}

impl PartialEq for ThermoGrid {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
            && self.cols == other.cols
            && self.data.len() == other.data.len()
            && self.data.iter().zip(other.data.iter()).all(|(a, b)| {
                match (a.is_nan(), b.is_nan()) {
                    (true, true) => true,
                    (false, false) => a == b,
                    _ => false,
                }
            })
    }
}

/// Per-cell accumulator for computing local temperature and pressure.
struct CellStats {
    kinetic: LocalKineticStats,
    /// Σ_pairs r_ij · F_ij, each pair counted once (half of it per ball).
    pair_virial: f32,
}

impl CellStats {
    fn new() -> Self {
        Self {
            kinetic: LocalKineticStats::new(),
            pair_virial: 0.0,
        }
    }
}

/// Virial pressure of a 2D region (force per unit length).
///
/// 2D virial theorem: `P·A = N_kin·T + (1/2)·Σ_pairs r_ij · F_ij`. The `1/2`
/// is `1/d` for dimension `d = 2` (it would be `1/3` in 3D) and is separate
/// from the double-counting factor already applied to `pair_virial`.
///
/// * `n_balls` - balls in the region; the kinetic term uses `N - 1` because
///   `temperature` is the centre-of-mass-subtracted temperature (see
///   `docs/done/nose-hoover.md`).
/// * `temperature` - COM-subtracted kinetic temperature (k_B = 1).
/// * `pair_virial` - `Σ_pairs r_ij · F_ij` with each pair counted once,
///   positive for repulsion.
/// * `area` - region area.
pub(crate) fn virial_pressure_2d(
    n_balls: usize,
    temperature: f32,
    pair_virial: f32,
    area: f32,
) -> f32 {
    const INV_DIMENSION: f32 = 0.5;
    let kinetic_term = (n_balls as f32 - 1.0) * temperature;
    (kinetic_term + INV_DIMENSION * pair_virial) / area
}

/// Target edge length of a thermo field cell, in reduced units (`σ` of
/// `N2`). The thermo field has a grid of its own (#290), not the physics
/// cell grid's, whose cells follow the largest species present: the overlay
/// keeps its resolution and its rolling average whatever species are in the
/// world. 10 gives the default 112 x 62 world 11 x 6 cells, its resolution
/// before #234.
pub const THERMO_CELL_SIZE: f32 = 10.0;

/// Thermo grid dimensions `(rows, cols)` for a world of `world_width` x
/// `world_height`: as many cells of at least [`THERMO_CELL_SIZE`] as fit,
/// at least one per axis. Depends on the world's size only, so a resize or a
/// load (a new world) is the only thing that changes it. Signs are ignored
/// (`Rect.h` may be negative).
pub fn thermo_grid_dims(world_width: f32, world_height: f32) -> (usize, usize) {
    let cells = |extent: f32| ((extent.abs() / THERMO_CELL_SIZE).floor() as usize).max(1);
    (cells(world_height), cells(world_width))
}

/// Compute per-cell temperature and pressure grids from ball state and force results.
///
/// `grid_rows` and `grid_cols` specify the grid dimensions; when either is
/// zero the function uses [`thermo_grid_dims`] of `world_rect`, the grid the
/// simulation records.
///
/// Returns `(temperature, pressure)` as flat row-major `ThermoGrid`s.
/// Cells with fewer than 2 balls contain `f32::NAN`.
pub fn compute_cell_thermo(
    balls: &[Ball2D],
    force_results: &[ForceResult],
    world_rect: &Rect,
    grid_rows: usize,
    grid_cols: usize,
) -> (ThermoGrid, ThermoGrid) {
    assert_eq!(
        balls.len(),
        force_results.len(),
        "balls and force_results must have the same length"
    );

    let ww = world_rect.w.abs();
    let wh = world_rect.h.abs();
    let (rows, cols) = if grid_rows == 0 || grid_cols == 0 {
        thermo_grid_dims(ww, wh)
    } else {
        (grid_rows, grid_cols)
    };
    let cell_w = ww / cols as f32;
    let cell_h = wh / rows as f32;

    // Top-left corner in world space (account for negative h)
    let origin_x = world_rect.x.min(world_rect.x + world_rect.w);
    let origin_y = world_rect.y.min(world_rect.y + world_rect.h);

    let mut stats: Vec<CellStats> = (0..rows * cols).map(|_| CellStats::new()).collect();

    for (ball, fr) in balls.iter().zip(force_results.iter()) {
        let pos = ball.get_position();
        let col = ((pos.x - origin_x) / cell_w).floor() as usize;
        let row = ((pos.y - origin_y) / cell_h).floor() as usize;
        let col = col.min(cols - 1);
        let row = row.min(rows - 1);
        let idx = row * cols + col;

        let cs = &mut stats[idx];
        cs.kinetic.add_ball(ball);
        cs.pair_virial += fr.virial * 0.5;
    }

    let cell_area = cell_w * cell_h;
    let mut temp_grid = ThermoGrid::filled(rows, cols, 0.0);
    let mut pres_grid = ThermoGrid::filled(rows, cols, 0.0);

    for r in 0..rows {
        for c in 0..cols {
            let cs = &stats[r * cols + c];
            let Some(t) = cs.kinetic.temperature() else {
                temp_grid.set(r, c, f32::NAN);
                pres_grid.set(r, c, f32::NAN);
                continue;
            };
            let p = virial_pressure_2d(cs.kinetic.count(), t, cs.pair_virial, cell_area);
            temp_grid.set(r, c, t);
            pres_grid.set(r, c, p);
        }
    }

    (temp_grid, pres_grid)
}

/// Steps between two T/P field samples (#229). Binning every ball into the
/// field grid costs O(balls) per sample; consecutive steps are strongly
/// correlated, so a sample every few steps averages to nearly the same field.
pub const THERMO_SAMPLE_STRIDE: usize = 10;

/// Steps the T/P field average spans. A larger window suppresses per-cell
/// noise from small-sample statistics.
pub const THERMO_WINDOW_STEPS: usize = 1000;

/// Samples kept for the average: the window in samples, one sample every
/// [`THERMO_SAMPLE_STRIDE`] steps, so it spans [`THERMO_WINDOW_STEPS`] steps.
pub const THERMO_WINDOW: usize = THERMO_WINDOW_STEPS / THERMO_SAMPLE_STRIDE;

/// Rolling-window averager for per-cell temperature and pressure grids.
///
/// Maintains a ring buffer of the last `window` samples (default
/// [`THERMO_WINDOW`]) and computes element-wise NaN-aware averages. The
/// window counts samples, not steps; the caller decides how often to sample
/// (see `SimulationEngine::record_thermo_fields_if_due`). When the pushed grid
/// shape differs from the stored shape (a resize), the history is cleared
/// and starts fresh; the shape does not depend on the species (#290).
pub struct ThermoFieldHistory {
    window: usize,
    t_buffer: VecDeque<ThermoGrid>,
    p_buffer: VecDeque<ThermoGrid>,
    shape: (usize, usize),
    t_sum: Vec<f32>,
    p_sum: Vec<f32>,
    t_count: Vec<u32>,
    p_count: Vec<u32>,
}

impl Default for ThermoFieldHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl ThermoFieldHistory {
    pub(crate) fn new() -> Self {
        Self::with_window(THERMO_WINDOW)
    }

    /// A history that averages over the last `window` samples.
    pub fn with_window(window: usize) -> Self {
        assert!(window > 0, "thermo window must hold at least one sample");
        Self {
            window,
            t_buffer: VecDeque::with_capacity(window),
            p_buffer: VecDeque::with_capacity(window),
            shape: (0, 0),
            t_sum: Vec::new(),
            p_sum: Vec::new(),
            t_count: Vec::new(),
            p_count: Vec::new(),
        }
    }

    /// Samples currently buffered (at most the window).
    pub fn len(&self) -> usize {
        self.t_buffer.len()
    }

    /// True if no sample is buffered (new, or just cleared).
    pub fn is_empty(&self) -> bool {
        self.t_buffer.is_empty()
    }

    /// Discard all buffered grids so the next `push` starts fresh.
    pub(crate) fn clear(&mut self) {
        self.t_buffer.clear();
        self.p_buffer.clear();
        self.shape = (0, 0);
        self.t_sum.clear();
        self.p_sum.clear();
        self.t_count.clear();
        self.p_count.clear();
    }

    /// Push a new pair of T/P grids into the ring buffer.
    ///
    /// If the grid shape differs from the stored shape, the entire history
    /// is cleared before inserting.
    pub fn push(&mut self, t_grid: ThermoGrid, p_grid: ThermoGrid) {
        let new_shape = t_grid.shape();
        let new_len = new_shape.0 * new_shape.1;
        debug_assert_eq!(p_grid.shape(), new_shape, "p_grid shape must match t_grid");
        debug_assert_eq!(
            t_grid.data.len(),
            new_shape.0 * new_shape.1,
            "t_grid data length must match rows*cols"
        );
        debug_assert_eq!(
            p_grid.data.len(),
            new_shape.0 * new_shape.1,
            "p_grid data length must match rows*cols"
        );

        if new_shape != self.shape {
            self.clear();
            self.shape = new_shape;
            self.reset_accumulators(new_len);
        } else if self.t_sum.len() != new_len {
            self.reset_accumulators(new_len);
        }

        if self.t_buffer.len() == self.window {
            if let Some(old_t) = self.t_buffer.pop_front() {
                Self::remove_grid(&old_t, &mut self.t_sum, &mut self.t_count);
            }
            if let Some(old_p) = self.p_buffer.pop_front() {
                Self::remove_grid(&old_p, &mut self.p_sum, &mut self.p_count);
            }
        }

        Self::add_grid(&t_grid, &mut self.t_sum, &mut self.t_count);
        Self::add_grid(&p_grid, &mut self.p_sum, &mut self.p_count);
        self.t_buffer.push_back(t_grid);
        self.p_buffer.push_back(p_grid);
    }

    /// Compute element-wise NaN-aware averages over the buffered grids.
    ///
    /// For each cell position, the average is taken over all stored samples
    /// where the value is finite. If every step is NaN at that position,
    /// the output is NaN.
    ///
    /// Returns `Some((temperature, pressure))` grids, or `None` if the
    /// buffer is empty (no data pushed yet, or cleared after shape change
    /// with no subsequent push).
    pub fn average(&self) -> Option<(ThermoGrid, ThermoGrid)> {
        if self.t_buffer.is_empty() {
            return None;
        }

        let (rows, cols) = self.shape;
        let avg_t = Self::average_from_accumulators(&self.t_sum, &self.t_count, rows, cols);
        let avg_p = Self::average_from_accumulators(&self.p_sum, &self.p_count, rows, cols);
        Some((avg_t, avg_p))
    }

    fn reset_accumulators(&mut self, len: usize) {
        self.t_sum = vec![0.0; len];
        self.p_sum = vec![0.0; len];
        self.t_count = vec![0; len];
        self.p_count = vec![0; len];
    }

    fn add_grid(grid: &ThermoGrid, sums: &mut [f32], counts: &mut [u32]) {
        for ((sum, count), value) in sums.iter_mut().zip(counts.iter_mut()).zip(grid.data.iter()) {
            if value.is_finite() {
                *sum += *value;
                *count += 1;
            }
        }
    }

    fn remove_grid(grid: &ThermoGrid, sums: &mut [f32], counts: &mut [u32]) {
        for ((sum, count), value) in sums.iter_mut().zip(counts.iter_mut()).zip(grid.data.iter()) {
            if value.is_finite() {
                *sum -= *value;
                *count = count.saturating_sub(1);
            }
        }
    }

    fn average_from_accumulators(
        sums: &[f32],
        counts: &[u32],
        rows: usize,
        cols: usize,
    ) -> ThermoGrid {
        let len = rows * cols;
        let mut data = vec![f32::NAN; len];
        for ((cell, sum), count) in data.iter_mut().zip(sums.iter()).zip(counts.iter()) {
            if *count > 0 {
                *cell = *sum / *count as f32;
            }
        }
        ThermoGrid { data, rows, cols }
    }
}
