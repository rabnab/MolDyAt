use crate::application::renderer_snapshot::ThermoGridSpec;
use crate::application::thermo_field::ThermoGrid;
use crate::graphics::renderer::interpolation::{interpolate_field, rainbow_colormap};

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

// ── Test 6: rainbow_colormap blue at 0 ───────────────────────────

#[test]
fn test_rainbow_colormap_blue_at_zero() {
    let [r, g, b, a] = rainbow_colormap(0.0);
    assert!((r - 0.0).abs() < 1e-5, "r={r}");
    assert!((g - 0.0).abs() < 1e-5, "g={g}");
    assert!((b - 1.0).abs() < 1e-5, "b={b}");
    assert!((a - 0.55).abs() < 1e-5, "a={a}");
}

// ── Test 7: rainbow_colormap red at 1 ────────────────────────────

#[test]
fn test_rainbow_colormap_red_at_one() {
    let [r, g, b, a] = rainbow_colormap(1.0);
    assert!((r - 1.0).abs() < 1e-5, "r={r}");
    assert!((g - 0.0).abs() < 1e-5, "g={g}");
    assert!((b - 0.0).abs() < 1e-5, "b={b}");
    assert!((a - 0.55).abs() < 1e-5, "a={a}");
}

// ── Test 8: rainbow_colormap midpoint in green/yellow band ───────

#[test]
fn test_rainbow_colormap_midpoint_green_yellow() {
    let [r, _g, b, _a] = rainbow_colormap(0.5);
    // At t=0.5 we're at cyan→green boundary: r low, b low
    assert!(r < 0.5, "r={r} should be low at midpoint");
    assert!(b < 0.5, "b={b} should be low at midpoint");
}

// ── Test 9: ThermoDisplayMode cycle correctness ──────────────────

#[test]
fn test_thermo_display_mode_cycle() {
    use crate::graphics::renderer::state_layers::ThermoDisplayMode;

    let mut mode = ThermoDisplayMode::Off;
    mode = mode.next(); // Off → Temperature
    assert_eq!(mode, ThermoDisplayMode::Temperature);
    mode = mode.next(); // Temperature → Pressure
    assert_eq!(mode, ThermoDisplayMode::Pressure);
    mode = mode.next(); // Pressure → Off
    assert_eq!(mode, ThermoDisplayMode::Off);

    // Two presses from Off lands on Pressure
    let mut mode2 = ThermoDisplayMode::Off;
    mode2 = mode2.next().next();
    assert_eq!(mode2, ThermoDisplayMode::Pressure);
}

// ── Test 10: No-data guard (None spec or 0×0 grid) ──────────────

#[test]
fn test_no_data_guard() {
    // Zero-size grid → no panic, returns NaN output
    let grid = ThermoGrid::filled(0, 0, 0.0);
    let spec = make_spec(0, 0);
    let out = interpolate_field(&grid, &spec, 10, 10);
    assert_eq!(out.len(), 100);
    assert!(out.iter().all(|v| v.is_nan()));
}

// ── Test 11: Uniform field guard (v_min == v_max) ────────────────

#[test]
fn test_uniform_field_guard() {
    // When all values are identical, v_min == v_max
    let grid = ThermoGrid::filled(4, 4, 42.0);
    let spec = make_spec(4, 4);
    let out = interpolate_field(&grid, &spec, 10, 10);
    // All values should be ≈ 42.0 (identical), so (v - v_min) / (v_max - v_min)
    // is 0/0 — the renderer should skip. Here we verify the field is finite
    // and uniform so the renderer can detect the guard condition.
    let v_min = out
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f32::INFINITY, f32::min);
    let v_max = out
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (v_max - v_min).abs() < 0.2,
        "uniform field: v_min={v_min}, v_max={v_max}"
    );
}
