use crate::wgpu::gpu::geometry::*;

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

#[test]
fn view_proj_maps_center_to_origin_and_edges_to_unit() {
    let m = build_view_proj_matrix(20.0, 10.0, 5.0, 2.0, 1.0);
    let (x, y) = apply_view_proj(m, 5.0, 2.0);
    assert!(approx(x, 0.0) && approx(y, 0.0));
    let (x, y) = apply_view_proj(m, 15.0, 7.0);
    assert!(approx(x, 1.0) && approx(y, 1.0));
}

#[test]
fn zoom_shrinks_the_visible_region() {
    let m = build_view_proj_matrix(20.0, 10.0, 0.0, 0.0, 2.0);
    let (x, y) = apply_view_proj(m, 5.0, 2.5);
    assert!(approx(x, 1.0) && approx(y, 1.0));
}

#[test]
fn mat4_mul_with_identity_is_noop() {
    let m = build_view_proj_matrix(20.0, 10.0, 3.0, -1.0, 1.5);
    assert_eq!(mat4_mul(&IDENTITY, &m), m);
    assert_eq!(mat4_mul(&m, &IDENTITY), m);
}

#[test]
fn px_to_ndc_flips_y() {
    assert_eq!(px_to_ndc(0.0, 0.0, 200.0, 100.0), [-1.0, 1.0]);
    assert_eq!(px_to_ndc(200.0, 100.0, 200.0, 100.0), [1.0, -1.0]);
    assert_eq!(px_to_ndc(100.0, 50.0, 200.0, 100.0), [0.0, 0.0]);
}
