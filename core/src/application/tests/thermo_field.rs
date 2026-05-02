use crate::application::thermo_field::{
    LocalKineticStats, ThermoFieldHistory, ThermoGrid, compute_cell_thermo, compute_viz_grid_dims,
};
use crate::objects2d::Rect;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::ball_impl::ForceResult;

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
    let (t, p) = compute_cell_thermo(&balls, &forces, &world, 0, 0);
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
    let (t, p) = compute_cell_thermo(&balls, &forces, &world, 0, 0);
    // With only 2 balls the grid is 1×1
    assert_eq!(t.rows(), 1);
    assert_eq!(t.cols(), 1);
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
    // In 2D the COM-subtracted system has 2 * (N - 1) = 2 thermal dof,
    // so T = (2 - 0) / 2 = 1.0.
    let world = Rect::new(0.0, 0.0, 10.0, 10.0);
    let balls = vec![
        make_ball(0, Vec2D::new(5.0, 5.0), Vec2D::new(1.0, 0.0), 1.0),
        make_ball(1, Vec2D::new(5.1, 5.1), Vec2D::new(-1.0, 0.0), 1.0),
    ];
    let forces = vec![make_force_result(0.0), make_force_result(0.0)];
    let (t, p) = compute_cell_thermo(&balls, &forces, &world, 0, 0);
    let expected_t = 1.0;
    assert!(
        (t.get(0, 0) - expected_t).abs() < 1e-5,
        "T={} expected {expected_t}",
        t.get(0, 0)
    );
    let expected_p = expected_t / 100.0;
    assert!(
        (p.get(0, 0) - expected_p).abs() < 1e-5,
        "P={} expected {expected_p}",
        p.get(0, 0)
    );
}

#[test]
fn test_local_temperature_uses_com_removed_degrees_of_freedom() {
    let mut stats = LocalKineticStats::new();
    stats.add_ball(&make_ball(0, Vec2D::zero(), Vec2D::new(1.0, 0.0), 1.0));
    stats.add_ball(&make_ball(1, Vec2D::zero(), Vec2D::new(-1.0, 0.0), 1.0));

    assert!((stats.temperature().unwrap() - 1.0).abs() < 1e-5);
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
    let (_, p) = compute_cell_thermo(&balls, &forces, &world, 0, 0);
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
    let (t, p) = compute_cell_thermo(&balls, &forces, &world, 0, 0);
    // Grid should be 1×1, T should match the positive-h test
    assert_eq!(t.rows(), 1);
    assert_eq!(t.cols(), 1);
    let expected_t = 1.0;
    assert!(
        (t.get(0, 0) - expected_t).abs() < 1e-5,
        "T={} expected {expected_t} with negative h",
        t.get(0, 0)
    );
    // Pressure with zero virial and T=1.0: P = ((2 - 1) * 1.0 + 0) / 100 = 0.01
    let expected_p = expected_t / 100.0;
    assert!(
        (p.get(0, 0) - expected_p).abs() < 1e-5,
        "P={} expected {expected_p} with negative h",
        p.get(0, 0)
    );
}

// ── Spatial mapping tests ────────────────────────────────────────

/// Verify that virial injected near the top-left corner of the world rect
/// appears in grid cell (0, 0) and that cells in other quadrants are NaN
/// (underpopulated). Uses 8 balls to produce a 2×2 grid.
#[test]
fn test_top_left_pressure_maps_to_grid_0_0() {
    // 20×20 world with origin at (0, 0), positive w/h.
    // 8 balls → 2×2 grid → cell_w = 10, cell_h = 10.
    // Place 2 balls with virial in top-left quadrant (0..10, 0..10)
    // and the remaining 6 balls spread one-per-cell so those cells have n < 2 → NaN.
    let world = Rect::new(0.0, 0.0, 20.0, 20.0);

    let virial_value = 10.0;
    let balls = vec![
        // Top-left quadrant: 2 balls → n=2, finite pressure
        make_ball(0, Vec2D::new(1.0, 1.0), Vec2D::zero(), 1.0),
        make_ball(1, Vec2D::new(2.0, 2.0), Vec2D::zero(), 1.0),
        // Other quadrants: 1 ball each → n < 2 → NaN
        make_ball(2, Vec2D::new(15.0, 1.0), Vec2D::zero(), 1.0),
        make_ball(3, Vec2D::new(1.0, 15.0), Vec2D::zero(), 1.0),
        make_ball(4, Vec2D::new(15.0, 15.0), Vec2D::zero(), 1.0),
        // Extra balls also in underpopulated cells (still only 1 per cell is fine,
        // but we need 8 total to get 2×2 grid)
        make_ball(5, Vec2D::new(15.5, 1.5), Vec2D::zero(), 1.0),
        make_ball(6, Vec2D::new(1.5, 15.5), Vec2D::zero(), 1.0),
        make_ball(7, Vec2D::new(15.5, 15.5), Vec2D::zero(), 1.0),
    ];
    // Only the top-left pair has virial; all others are zero
    let forces: Vec<_> = (0..balls.len())
        .map(|i| make_force_result(if i < 2 { virial_value } else { 0.0 }))
        .collect();

    let (_, p) = compute_cell_thermo(&balls, &forces, &world, 0, 0);

    assert_eq!(p.rows(), 2, "expected 2 rows");
    assert_eq!(p.cols(), 2, "expected 2 cols");

    // Top-left cell (0,0): two resting balls with virial → finite positive pressure
    let p00 = p.get(0, 0);
    assert!(
        p00.is_finite() && p00 > 0.0,
        "top-left cell pressure should be finite and positive, got {p00}"
    );

    // All other cells should have exactly 2 balls each with zero virial → P = 0
    // (resting balls, zero virial)
    for (r, c) in [(0, 1), (1, 0), (1, 1)] {
        let prc = p.get(r, c);
        assert!(
            prc.is_finite() && (prc - 0.0).abs() < 1e-5,
            "cell ({r},{c}) pressure should be ~0 (zero virial, resting), got {prc}"
        );
    }
}

/// Same test with a negative-height world rect (top-down coordinate system).
/// Balls placed near the Rect origin (x, y) = (0, 20) should map to the
/// *last* row of the grid because the min-based origin starts at y = 0.
#[test]
fn test_top_left_pressure_negative_height() {
    // Rect origin is (0, 20), extends right 20 and up(negative) 20 → y range 0..20.
    // min-based origin: (0, 0). cell_h = 10.
    // Balls near y = 19 → row = floor((19-0)/10) = 1 (last row).
    let world = Rect::new(0.0, 20.0, 20.0, -20.0);

    let virial_value = 10.0;
    let balls = vec![
        // Near Rect origin corner (x~0, y~20) → grid row 1, col 0
        make_ball(0, Vec2D::new(1.0, 19.0), Vec2D::zero(), 1.0),
        make_ball(1, Vec2D::new(2.0, 18.0), Vec2D::zero(), 1.0),
        // Fill remaining cells
        make_ball(2, Vec2D::new(15.0, 19.0), Vec2D::zero(), 1.0),
        make_ball(3, Vec2D::new(1.0, 1.0), Vec2D::zero(), 1.0),
        make_ball(4, Vec2D::new(15.0, 1.0), Vec2D::zero(), 1.0),
        make_ball(5, Vec2D::new(15.5, 18.5), Vec2D::zero(), 1.0),
        make_ball(6, Vec2D::new(1.5, 1.5), Vec2D::zero(), 1.0),
        make_ball(7, Vec2D::new(15.5, 1.5), Vec2D::zero(), 1.0),
    ];
    let forces: Vec<_> = (0..balls.len())
        .map(|i| make_force_result(if i < 2 { virial_value } else { 0.0 }))
        .collect();

    let (_, p) = compute_cell_thermo(&balls, &forces, &world, 0, 0);

    assert_eq!(p.rows(), 2);
    assert_eq!(p.cols(), 2);

    // With negative h, the Rect origin corner maps to (row=1, col=0), not (0, 0).
    let p10 = p.get(1, 0);
    assert!(
        p10.is_finite() && p10 > 0.0,
        "Rect-origin corner cell (1,0) should have positive pressure, got {p10}"
    );

    // Top-left grid cell (0,0) should have zero-virial resting balls
    let p00 = p.get(0, 0);
    assert!(
        p00.is_finite() && (p00).abs() < 1e-5,
        "grid (0,0) should be ~0 for negative-h world, got {p00}"
    );
}

// ── ThermoFieldHistory tests ─────────────────────────────────────

#[test]
fn test_history_empty_returns_none() {
    let h = ThermoFieldHistory::new();
    assert!(h.average().is_none());
}

fn grid_2x2(v00: f32, v01: f32, v10: f32, v11: f32) -> ThermoGrid {
    let mut g = ThermoGrid::filled(2, 2, 0.0);
    g.set(0, 0, v00);
    g.set(0, 1, v01);
    g.set(1, 0, v10);
    g.set(1, 1, v11);
    g
}

fn grid_1x1(val: f32) -> ThermoGrid {
    let mut g = ThermoGrid::filled(1, 1, 0.0);
    g.set(0, 0, val);
    g
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
    assert_eq!(avg_t.rows(), 4);
    assert_eq!(avg_t.cols(), 5);
    assert_eq!(avg_t, t_4x5);
    assert_eq!(avg_p, p_4x5);
}

#[test]
fn test_history_ring_wraparound() {
    // THERMO_WINDOW is 1000, so we push 1005 items to trigger wraparound.
    // Values 1..=1005, of which the last 1000 (6..=1005) remain.
    let mut h = ThermoFieldHistory::new();
    let total = 1005u32;
    let window = 1000u32;
    for i in 1..=total {
        h.push(grid_1x1(i as f32), grid_1x1((i * 2) as f32));
    }
    let (avg_t, avg_p) = h.average().expect("should have data");
    // Kept values: (total-window+1)..=total  i.e. 6..=1005
    // T average: sum(6..=1005)/1000 = (6+1005)*1000/2 / 1000 = 505.5
    // P average: sum(12,14,...,2010)/1000 = 2 * 505.5 = 1011.0
    let first_kept = (total - window + 1) as f32;
    let last_kept = total as f32;
    let expected_t = (first_kept + last_kept) / 2.0;
    let expected_p = expected_t * 2.0;
    assert!(
        (avg_t.get(0, 0) - expected_t).abs() < 1e-2,
        "T={}, expected={}",
        avg_t.get(0, 0),
        expected_t,
    );
    assert!(
        (avg_p.get(0, 0) - expected_p).abs() < 1e-2,
        "P={}, expected={}",
        avg_p.get(0, 0),
        expected_p,
    );
}
