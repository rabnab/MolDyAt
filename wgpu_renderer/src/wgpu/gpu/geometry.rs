//! Small coordinate helpers shared by the pipelines (CPU side).
//!
//! Matrices are column-major `[[f32; 4]; 4]`, as uploaded to WGSL `mat4x4<f32>`.

use rust_thermo_core::application::renderer_snapshot::RendererSnapshot;

pub const IDENTITY: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Orthographic 2D view-projection: the visible world region
/// (`world_width` × `world_height`, divided by `zoom`) centred on
/// `(center_x, center_y)` maps to NDC `[-1, 1]²`.
pub fn build_view_proj_matrix(
    world_width: f32,
    world_height: f32,
    center_x: f32,
    center_y: f32,
    zoom: f32,
) -> [[f32; 4]; 4] {
    let scale_x = 2.0 * zoom / world_width;
    let scale_y = 2.0 * zoom / world_height;
    [
        [scale_x, 0.0, 0.0, 0.0],
        [0.0, scale_y, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-center_x * scale_x, -center_y * scale_y, 0.0, 1.0],
    ]
}

/// `a * b` for column-major 4×4 matrices.
pub fn mat4_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            out[col][row] = (0..4).map(|k| a[k][row] * b[col][k]).sum();
        }
    }
    out
}

/// Transform a world point to NDC on the CPU.
pub fn apply_view_proj(m: [[f32; 4]; 4], wx: f32, wy: f32) -> (f32, f32) {
    let x = m[0][0] * wx + m[1][0] * wy + m[3][0];
    let y = m[0][1] * wx + m[1][1] * wy + m[3][1];
    let w = m[0][3] * wx + m[1][3] * wy + m[3][3];
    (x / w, y / w)
}

/// Window pixel coordinates (origin top-left, y down) to NDC (y up).
pub fn px_to_ndc(x_px: f32, y_px: f32, win_w: f32, win_h: f32) -> [f32; 2] {
    [(x_px / win_w) * 2.0 - 1.0, 1.0 - (y_px / win_h) * 2.0]
}

/// Larger side of the simulation world (see `Rect::extent`).
pub fn world_extent(snapshot: &RendererSnapshot) -> f32 {
    snapshot.cell_grid_bounds.extent()
}
