use crate::objects2d::ball_impl::{Ball2D, ForceResult};
use crate::objects2d::{Rect, Vec2D};
use std::collections::VecDeque;

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

    /// Create a grid filled with NaN.
    pub fn nan(rows: usize, cols: usize) -> Self {
        Self::filled(rows, cols, f32::NAN)
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

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
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
/// `grid_rows` and `grid_cols` specify the desired grid dimensions (e.g. from the
/// physics `CellGrid`).  When either is zero the function falls back to
/// [`compute_viz_grid_dims`] so behaviour is unchanged during start-up before the
/// cell grid is fully initialised.
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
        compute_viz_grid_dims(balls.len(), ww, wh)
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
/// A larger window suppresses per-cell noise from small-sample statistics.
const THERMO_WINDOW: usize = 1000;

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

    /// Discard all buffered grids so the next `push` starts fresh.
    pub fn clear(&mut self) {
        self.t_buffer.clear();
        self.p_buffer.clear();
        self.shape = (0, 0);
    }

    /// Push a new pair of T/P grids into the ring buffer.
    ///
    /// If the grid shape differs from the stored shape, the entire history
    /// is cleared before inserting.
    pub fn push(&mut self, t_grid: ThermoGrid, p_grid: ThermoGrid) {
        let new_shape = t_grid.shape();
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
        for (i, cell) in data.iter_mut().enumerate() {
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
