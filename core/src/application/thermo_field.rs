use crate::objects2d::ball_impl::{Ball2D, ForceResult};
use crate::objects2d::{Rect, Vec2D};

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

/// Compute the visualization grid dimensions (rows, cols) such that the average
/// cell occupancy exceeds 2 balls and the grid aspect ratio matches the world.
///
/// Returns `(rows, cols)` with both >= 1.
pub fn compute_viz_grid_dims(
    n_balls: usize,
    world_width: f32,
    world_height: f32,
) -> (usize, usize) {
    if n_balls < 2 || world_width <= 0.0 || world_height <= 0.0 {
        return (1, 1);
    }
    let aspect = world_height / world_width;
    let cols = ((n_balls as f32 / (2.0 * aspect)).sqrt().floor() as usize).max(1);
    let rows = ((cols as f32 * aspect).floor() as usize).max(1);
    (rows, cols)
}

/// Compute per-cell temperature and pressure grids from ball state and force results.
///
/// Returns `(temperature, pressure)` as row-major `Vec<Vec<f32>>` of shape `[rows][cols]`.
/// Cells with fewer than 2 balls contain `f32::NAN`.
pub fn compute_cell_thermo(
    balls: &[Ball2D],
    force_results: &[ForceResult],
    world_rect: &Rect,
) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let (rows, cols) = compute_viz_grid_dims(balls.len(), world_rect.w, world_rect.h);
    let cell_w = world_rect.w / cols as f32;
    let cell_h = world_rect.h / rows as f32;

    let mut stats: Vec<CellStats> = (0..rows * cols).map(|_| CellStats::new()).collect();

    for (i, ball) in balls.iter().enumerate() {
        let pos = ball.get_position();
        let col = ((pos.x - world_rect.x) / cell_w).floor() as usize;
        let row = ((pos.y - world_rect.y) / cell_h).floor() as usize;
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
        cs.virial += force_results[i].virial * 0.5;
    }

    let cell_area = cell_w * cell_h;
    let mut temp_grid = vec![vec![0.0f32; cols]; rows];
    let mut pres_grid = vec![vec![0.0f32; cols]; rows];

    for r in 0..rows {
        for c in 0..cols {
            let cs = &stats[r * cols + c];
            if cs.n < 2 {
                temp_grid[r][c] = f32::NAN;
                pres_grid[r][c] = f32::NAN;
                continue;
            }
            let n = cs.n as f32;
            let t = (cs.sum_mv2 - cs.sum_mv.norm_sq() / cs.sum_m) / (2.0 * n);
            let p = (n * t + cs.virial) / cell_area;
            temp_grid[r][c] = t;
            pres_grid[r][c] = p;
        }
    }

    (temp_grid, pres_grid)
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
        ];
        for (n, w, h) in cases {
            let (rows, cols) = compute_viz_grid_dims(n, w, h);
            assert!(rows >= 1, "rows must be >= 1 for N={n}, W={w}, H={h}");
            assert!(cols >= 1, "cols must be >= 1 for N={n}, W={w}, H={h}");
            let cells = rows * cols;
            assert!(
                n as f32 / cells as f32 > 2.0,
                "average occupancy must be > 2: N={n}, cells={cells}"
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

    // ── compute_cell_thermo tests ────────────────────────────────────

    #[test]
    fn test_sparse_cell_returns_nan() {
        // Single ball → the one cell it lands in has n=1 → NaN
        let world = Rect::new(0.0, 0.0, 10.0, 10.0);
        let balls = vec![make_ball(0, Vec2D::new(5.0, 5.0), Vec2D::zero(), 1.0)];
        let forces = vec![make_force_result(0.0)];
        let (t, p) = compute_cell_thermo(&balls, &forces, &world);
        assert!(t[0][0].is_nan(), "T should be NaN for n < 2");
        assert!(p[0][0].is_nan(), "P should be NaN for n < 2");
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
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].len(), 1);
        assert_eq!(t[0][0], 0.0, "T should be 0 for resting balls");
        assert_eq!(
            p[0][0], 0.0,
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
            (t[0][0] - expected_t).abs() < 1e-5,
            "T={} expected {expected_t}",
            t[0][0]
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
            (p[0][0] - expected_p).abs() < 1e-5,
            "P={} expected {expected_p}",
            p[0][0]
        );
    }
}
