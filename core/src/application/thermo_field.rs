use crate::objects2d::ball_impl::{Ball2D, ForceResult};
use crate::objects2d::{Rect, Vec2D};
use std::collections::VecDeque;

/// A flat, row-major grid of `f32` values with explicit dimensions.
///
/// Single contiguous allocation for cache-friendly iteration. Used for
/// per-cell temperature and pressure fields.
#[derive(Clone, Debug)]
pub struct ThermoGrid {
    pub data: Vec<f32>,
    pub rows: usize,
    pub cols: usize,
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

    /// Create a grid filled with NaN.
    pub fn nan(rows: usize, cols: usize) -> Self {
        Self::filled(rows, cols, f32::NAN)
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> f32 {
        self.data[r * self.cols + c]
    }

    #[inline]
    pub fn set(&mut self, r: usize, c: usize, val: f32) {
        self.data[r * self.cols + c] = val;
    }

    /// Returns `(rows, cols)`.
    pub fn shape(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }
}

impl PartialEq for ThermoGrid {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
            && self.cols == other.cols
            && self.data.len() == other.data.len()
            && self
                .data
                .iter()
                .zip(other.data.iter())
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

/// Per-cell accumulator for computing local temperature and pressure.
struct CellStats {
    n: usize,
    sum_m: f32,
    sum_mv: Vec2D,
    sum_mv2: f32,
    virial: f32,
}

impl CellStats {
    fn new() -> Self {
        Self {
            n: 0,
            sum_m: 0.0,
            sum_mv: Vec2D::zero(),
            sum_mv2: 0.0,
            virial: 0.0,
        }
    }
}

/// Compute the visualization grid dimensions (rows, cols) aiming for an average
/// cell occupancy of roughly 2 balls while matching the world aspect ratio.
///
/// This is a heuristic: extreme aspect ratios or small ball counts may produce
/// grids where the actual average occupancy falls below 2.
///
/// Uses absolute values of width/height to handle top-down coordinate systems
/// where `Rect.h` may be negative.
///
/// Returns `(rows, cols)` with both >= 1.
pub fn compute_viz_grid_dims(
    n_balls: usize,
    world_width: f32,
    world_height: f32,
) -> (usize, usize) {
    let ww = world_width.abs();
    let wh = world_height.abs();
    if n_balls < 2 || ww == 0.0 || wh == 0.0 {
        return (1, 1);
    }
    let aspect = wh / ww;
    let cols = ((n_balls as f32 / (2.0 * aspect)).sqrt().floor() as usize).max(1);
    let rows = ((cols as f32 * aspect).floor() as usize).max(1);
    (rows, cols)
}

/// Compute per-cell temperature and pressure grids from ball state and force results.
///
/// Returns `(temperature, pressure)` as flat row-major `ThermoGrid`s.
/// Cells with fewer than 2 balls contain `f32::NAN`.
pub fn compute_cell_thermo(
    balls: &[Ball2D],
    force_results: &[ForceResult],
    world_rect: &Rect,
) -> (ThermoGrid, ThermoGrid) {
    assert_eq!(
        balls.len(),
        force_results.len(),
        "balls and force_results must have the same length"
    );

    let ww = world_rect.w.abs();
    let wh = world_rect.h.abs();
    let (rows, cols) = compute_viz_grid_dims(balls.len(), ww, wh);
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

        let m = ball.mass;
        let v = &ball.v;
        let cs = &mut stats[idx];
        cs.n += 1;
        cs.sum_m += m;
        cs.sum_mv.x += m * v.x;
        cs.sum_mv.y += m * v.y;
        cs.sum_mv2 += m * v.norm_sq();
        cs.virial += fr.virial * 0.5;
    }

    let cell_area = cell_w * cell_h;
    let mut temp_grid = ThermoGrid::filled(rows, cols, 0.0);
    let mut pres_grid = ThermoGrid::filled(rows, cols, 0.0);

    for r in 0..rows {
        for c in 0..cols {
            let cs = &stats[r * cols + c];
            if cs.n < 2 {
                temp_grid.set(r, c, f32::NAN);
                pres_grid.set(r, c, f32::NAN);
                continue;
            }
            let n = cs.n as f32;
            let t = (cs.sum_mv2 - cs.sum_mv.norm_sq() / cs.sum_m) / (2.0 * n);
            let p = (n * t + cs.virial) / cell_area;
            temp_grid.set(r, c, t);
            pres_grid.set(r, c, p);
        }
    }

    (temp_grid, pres_grid)
}

/// Number of simulation steps to average over for T/P field smoothing.
const THERMO_WINDOW: usize = 10;

/// Rolling-window averager for per-cell temperature and pressure grids.
///
/// Maintains a ring buffer of the last [`THERMO_WINDOW`] grids and computes
/// element-wise NaN-aware averages. When the pushed grid shape differs from
/// the stored shape, the history is cleared and starts fresh.
pub struct ThermoFieldHistory {
    t_buffer: VecDeque<ThermoGrid>,
    p_buffer: VecDeque<ThermoGrid>,
    shape: (usize, usize),
}

impl Default for ThermoFieldHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl ThermoFieldHistory {
    pub fn new() -> Self {
        Self {
            t_buffer: VecDeque::with_capacity(THERMO_WINDOW),
            p_buffer: VecDeque::with_capacity(THERMO_WINDOW),
            shape: (0, 0),
        }
    }

    /// Push a new pair of T/P grids into the ring buffer.
    ///
    /// If the grid shape differs from the stored shape, the entire history
    /// is cleared before inserting.
    pub fn push(&mut self, t_grid: ThermoGrid, p_grid: ThermoGrid) {
        let new_shape = t_grid.shape();
        debug_assert_eq!(p_grid.shape(), new_shape, "p_grid shape must match t_grid");

        if new_shape != self.shape {
            self.t_buffer.clear();
            self.p_buffer.clear();
            self.shape = new_shape;
        }

        if self.t_buffer.len() == THERMO_WINDOW {
            self.t_buffer.pop_front();
            self.p_buffer.pop_front();
        }

        self.t_buffer.push_back(t_grid);
        self.p_buffer.push_back(p_grid);
    }

    /// Compute element-wise NaN-aware averages over the buffered grids.
    ///
    /// For each cell position, the average is taken over all stored steps
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
        let avg_t = Self::average_buffer(&self.t_buffer, rows, cols);
        let avg_p = Self::average_buffer(&self.p_buffer, rows, cols);
        Some((avg_t, avg_p))
    }

    fn average_buffer(buffer: &VecDeque<ThermoGrid>, rows: usize, cols: usize) -> ThermoGrid {
        let len = rows * cols;
        let mut data = vec![f32::NAN; len];
        for (i, cell) in data.iter_mut().enumerate().take(len) {
            let mut sum = 0.0f32;
            let mut count = 0u32;
            for grid in buffer {
                let val = grid.data[i];
                if val.is_finite() {
                    sum += val;
                    count += 1;
                }
            }
            if count > 0 {
                *cell = sum / count as f32;
            }
        }
        ThermoGrid { data, rows, cols }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects2d::Vec2D;
    use crate::objects2d::ball_impl::Ball2D;

    fn make_force_result(virial: f32) -> ForceResult {
        ForceResult {
            min_dist: f32::NAN,
            force: Vec2D::zero(),
            potential_energy: 0.0,
            neighbor_forces: Vec::new(),
            virial,
        }
    }

    fn make_ball(id: usize, pos: Vec2D, velocity: Vec2D, mass: f32) -> Ball2D {
        let mut b = Ball2D::new(id, pos, (0, 1.0));
        b.v = velocity;
        b.mass = mass;
        b
    }

    // ── compute_viz_grid_dims tests ──────────────────────────────────

    #[test]
    fn test_grid_dims_minimum_one() {
        assert_eq!(compute_viz_grid_dims(0, 10.0, 10.0), (1, 1));
        assert_eq!(compute_viz_grid_dims(1, 10.0, 10.0), (1, 1));
    }

    #[test]
    fn test_grid_dims_invariants() {
        let cases: Vec<(usize, f32, f32)> = vec![
            (100, 10.0, 10.0),
            (200, 20.0, 10.0),
            (500, 10.0, 30.0),
            (1000, 50.0, 50.0),
            (50, 5.0, 20.0),
            // High aspect ratio (H/W = 40) to exercise extreme shapes
            (1000, 5.0, 200.0),
        ];
        for (n, w, h) in cases {
            let (rows, cols) = compute_viz_grid_dims(n, w, h);
            assert!(rows >= 1, "rows must be >= 1 for N={n}, W={w}, H={h}");
            assert!(cols >= 1, "cols must be >= 1 for N={n}, W={w}, H={h}");
            let cells = rows * cols;
            // Occupancy >= 2 is a heuristic goal, not a hard guarantee.
            // Extreme aspect ratios or small N may violate this.
            let occupancy = n as f32 / cells as f32;
            assert!(
                occupancy >= 1.0,
                "occupancy must be >= 1: N={n}, cells={cells}, occ={occupancy}"
            );
            let aspect_ratio = rows as f32 / cols as f32;
            let world_aspect = h / w;
            let rel_err = (aspect_ratio - world_aspect).abs() / world_aspect;
            assert!(
                rel_err < 0.5,
                "aspect mismatch: grid={aspect_ratio:.2}, world={world_aspect:.2}, err={rel_err:.2} for N={n}"
            );
        }
    }

    #[test]
    fn test_grid_dims_negative_height() {
        // Production uses negative h for top-down coords
        let (rows, cols) = compute_viz_grid_dims(100, 10.0, -10.0);
        assert!(rows >= 1);
        assert!(cols >= 1);
        assert!(100.0 / (rows * cols) as f32 >= 2.0);
        // Should give the same result as positive h
        assert_eq!((rows, cols), compute_viz_grid_dims(100, 10.0, 10.0));
    }

    // ── compute_cell_thermo tests ────────────────────────────────────

    #[test]
    fn test_sparse_cell_returns_nan() {
        // Single ball → the one cell it lands in has n=1 → NaN
        let world = Rect::new(0.0, 0.0, 10.0, 10.0);
        let balls = vec![make_ball(0, Vec2D::new(5.0, 5.0), Vec2D::zero(), 1.0)];
        let forces = vec![make_force_result(0.0)];
        let (t, p) = compute_cell_thermo(&balls, &forces, &world);
        assert!(t.get(0, 0).is_nan(), "T should be NaN for n < 2");
        assert!(p.get(0, 0).is_nan(), "P should be NaN for n < 2");
    }

    #[test]
    fn test_two_resting_balls_zero_tp() {
        // Two resting balls (v=0, virial=0) in one cell → T=0, P=0
        let world = Rect::new(0.0, 0.0, 10.0, 10.0);
        let balls = vec![
            make_ball(0, Vec2D::new(5.0, 5.0), Vec2D::zero(), 1.0),
            make_ball(1, Vec2D::new(5.1, 5.1), Vec2D::zero(), 1.0),
        ];
        let forces = vec![make_force_result(0.0), make_force_result(0.0)];
        let (t, p) = compute_cell_thermo(&balls, &forces, &world);
        // With only 2 balls the grid is 1×1
        assert_eq!(t.rows, 1);
        assert_eq!(t.cols, 1);
        assert_eq!(t.get(0, 0), 0.0, "T should be 0 for resting balls");
        assert_eq!(
            p.get(0, 0),
            0.0,
            "P should be 0 for resting balls with no virial"
        );
    }

    #[test]
    fn test_known_velocities_no_virial() {
        // Two balls with known velocities, same mass, no virial → verify T analytically.
        // Ball A: v=(1, 0), m=1.  Ball B: v=(-1, 0), m=1.
        // COM velocity = (0, 0).
        // sum_mv2 = 1*1 + 1*1 = 2
        // sum_mv = (0, 0) → sum_mv.norm_sq()/sum_m = 0
        // T = (2 - 0) / (2 * 2) = 0.5
        let world = Rect::new(0.0, 0.0, 10.0, 10.0);
        let balls = vec![
            make_ball(0, Vec2D::new(5.0, 5.0), Vec2D::new(1.0, 0.0), 1.0),
            make_ball(1, Vec2D::new(5.1, 5.1), Vec2D::new(-1.0, 0.0), 1.0),
        ];
        let forces = vec![make_force_result(0.0), make_force_result(0.0)];
        let (t, _) = compute_cell_thermo(&balls, &forces, &world);
        let expected_t = 0.5;
        assert!(
            (t.get(0, 0) - expected_t).abs() < 1e-5,
            "T={} expected {expected_t}",
            t.get(0, 0)
        );
    }

    #[test]
    fn test_known_virial_pressure() {
        // Two resting balls (T=0) with injected virial → P = virial / cell_area.
        // virial per ball = 10.0 → accumulated as 10.0*0.5 = 5.0 per ball → total = 10.0
        // Grid 1×1 over 10×10 world → cell_area = 100
        // P = (2*0 + 10.0) / 100 = 0.1
        let world = Rect::new(0.0, 0.0, 10.0, 10.0);
        let balls = vec![
            make_ball(0, Vec2D::new(5.0, 5.0), Vec2D::zero(), 1.0),
            make_ball(1, Vec2D::new(5.1, 5.1), Vec2D::zero(), 1.0),
        ];
        let forces = vec![make_force_result(10.0), make_force_result(10.0)];
        let (_, p) = compute_cell_thermo(&balls, &forces, &world);
        let expected_p = 10.0 / 100.0;
        assert!(
            (p.get(0, 0) - expected_p).abs() < 1e-5,
            "P={} expected {expected_p}",
            p.get(0, 0)
        );
    }

    #[test]
    fn test_negative_height_world_rect() {
        // Production uses Rect with negative h (top-down coordinate system).
        // e.g. Rect::new(-5.0, 5.0, 10.0, -10.0) → world from (-5, -5) to (5, 5)
        let world = Rect::new(-5.0, 5.0, 10.0, -10.0);
        let balls = vec![
            make_ball(0, Vec2D::new(0.0, 0.0), Vec2D::new(1.0, 0.0), 1.0),
            make_ball(1, Vec2D::new(0.1, 0.1), Vec2D::new(-1.0, 0.0), 1.0),
        ];
        let forces = vec![make_force_result(0.0), make_force_result(0.0)];
        let (t, p) = compute_cell_thermo(&balls, &forces, &world);
        // Grid should be 1×1, T should match the positive-h test
        assert_eq!(t.rows, 1);
        assert_eq!(t.cols, 1);
        let expected_t = 0.5;
        assert!(
            (t.get(0, 0) - expected_t).abs() < 1e-5,
            "T={} expected {expected_t} with negative h",
            t.get(0, 0)
        );
        // Pressure with zero virial and T=0.5: P = (2*0.5 + 0) / 100 = 0.01
        let expected_p = (2.0 * expected_t) / 100.0;
        assert!(
            (p.get(0, 0) - expected_p).abs() < 1e-5,
            "P={} expected {expected_p} with negative h",
            p.get(0, 0)
        );
    }

    // ── ThermoFieldHistory tests ─────────────────────────────────────

    #[test]
    fn test_history_empty_returns_none() {
        let h = ThermoFieldHistory::new();
        assert!(h.average().is_none());
    }

    fn grid_2x2(v00: f32, v01: f32, v10: f32, v11: f32) -> ThermoGrid {
        ThermoGrid {
            data: vec![v00, v01, v10, v11],
            rows: 2,
            cols: 2,
        }
    }

    fn grid_1x1(val: f32) -> ThermoGrid {
        ThermoGrid {
            data: vec![val],
            rows: 1,
            cols: 1,
        }
    }

    #[test]
    fn test_history_single_push() {
        let mut h = ThermoFieldHistory::new();
        let t = grid_2x2(1.0, 2.0, 3.0, 4.0);
        let p = grid_2x2(5.0, 6.0, 7.0, 8.0);
        h.push(t.clone(), p.clone());
        let (avg_t, avg_p) = h.average().expect("should have data");
        assert_eq!(avg_t, t);
        assert_eq!(avg_p, p);
    }

    #[test]
    fn test_history_steady_state() {
        let mut h = ThermoFieldHistory::new();
        let t = grid_2x2(1.0, 2.0, 3.0, 4.0);
        let p = grid_2x2(5.0, 6.0, 7.0, 8.0);
        for _ in 0..10 {
            h.push(t.clone(), p.clone());
        }
        let (avg_t, avg_p) = h.average().expect("should have data");
        assert_eq!(avg_t, t);
        assert_eq!(avg_p, p);
    }

    #[test]
    fn test_history_partial_window() {
        let mut h = ThermoFieldHistory::new();
        // Push 3 grids: values 1, 2, 3 at [0][0]
        for i in 1..=3 {
            h.push(grid_1x1(i as f32), grid_1x1((i * 10) as f32));
        }
        let (avg_t, avg_p) = h.average().expect("should have data");
        // Average of 1,2,3 = 2.0; average of 10,20,30 = 20.0
        assert!((avg_t.get(0, 0) - 2.0).abs() < 1e-5);
        assert!((avg_p.get(0, 0) - 20.0).abs() < 1e-5);
    }

    #[test]
    fn test_history_nan_handling() {
        let mut h = ThermoFieldHistory::new();
        // Step 1: finite value
        h.push(grid_1x1(4.0), grid_1x1(8.0));
        // Step 2: NaN
        h.push(grid_1x1(f32::NAN), grid_1x1(f32::NAN));
        // Step 3: finite value
        h.push(grid_1x1(6.0), grid_1x1(12.0));
        let (avg_t, avg_p) = h.average().expect("should have data");
        // Average over 2 finite values: (4+6)/2 = 5, (8+12)/2 = 10
        assert!((avg_t.get(0, 0) - 5.0).abs() < 1e-5);
        assert!((avg_p.get(0, 0) - 10.0).abs() < 1e-5);
    }

    #[test]
    fn test_history_shape_change_clears() {
        let mut h = ThermoFieldHistory::new();
        // Push a 3×4 grid
        h.push(ThermoGrid::filled(3, 4, 1.0), ThermoGrid::filled(3, 4, 2.0));

        // Push a 4×5 grid with different values
        let t_4x5 = ThermoGrid::filled(4, 5, 10.0);
        let p_4x5 = ThermoGrid::filled(4, 5, 20.0);
        h.push(t_4x5.clone(), p_4x5.clone());

        let (avg_t, avg_p) = h.average().expect("should have data");
        // Only the 4×5 grid should remain
        assert_eq!(avg_t.rows, 4);
        assert_eq!(avg_t.cols, 5);
        assert_eq!(avg_t, t_4x5);
        assert_eq!(avg_p, p_4x5);
    }

    #[test]
    fn test_history_ring_wraparound() {
        let mut h = ThermoFieldHistory::new();
        // Push 15 grids with values 1..=15
        for i in 1..=15 {
            h.push(grid_1x1(i as f32), grid_1x1((i * 2) as f32));
        }
        let (avg_t, avg_p) = h.average().expect("should have data");
        // Only the last 10 (values 6..=15) should be in the buffer
        // T average: (6+7+8+9+10+11+12+13+14+15)/10 = 105/10 = 10.5
        // P average: (12+14+16+18+20+22+24+26+28+30)/10 = 210/10 = 21.0
        assert!(
            (avg_t.get(0, 0) - 10.5).abs() < 1e-5,
            "T={}",
            avg_t.get(0, 0)
        );
        assert!(
            (avg_p.get(0, 0) - 21.0).abs() < 1e-5,
            "P={}",
            avg_p.get(0, 0)
        );
    }
}
