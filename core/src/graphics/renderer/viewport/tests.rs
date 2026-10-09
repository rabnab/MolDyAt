// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

#[test]
fn test_viewport_transform_new() {
    let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
    let transform = ViewportTransform::new(world, (800.0, 600.0), Margins::symmetric(50.0, 50.0));

    assert!(approx_eq(transform.zoom(), 1.0, 1e-6));
    assert!(approx_eq(transform.pan_offset().x, 0.0, 1e-6));
    assert!(approx_eq(transform.pan_offset().y, 0.0, 1e-6));

    let viewport_size = transform.viewport_size_px();
    assert!(approx_eq(viewport_size.0, 700.0, 1e-6));
    assert!(approx_eq(viewport_size.1, 500.0, 1e-6));
}

#[test]
fn test_viewport_transform_zoom() {
    let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
    let mut transform =
        ViewportTransform::new(world, (800.0, 600.0), Margins::symmetric(50.0, 50.0));

    let point = Vec2D::zero();
    transform.zoom_at_point(&point, 2.0);

    assert!(approx_eq(transform.zoom(), 2.0, 1e-6));
}

#[test]
fn test_viewport_transform_coordinates() {
    let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
    let transform = ViewportTransform::new(world, (400.0, 400.0), Margins::symmetric(0.0, 0.0));

    // At 1x zoom, viewport center (200, 200) should map to world center (0, 0)
    let world_pos = transform.px_to_world(&[200.0, 200.0]);
    println!("px (200, 200) -> world ({}, {})", world_pos.x, world_pos.y);
    assert!(approx_eq(world_pos.x, 0.0, 1e-2));
    assert!(approx_eq(world_pos.y, 0.0, 1e-2));

    // Convert back
    let px_pos = transform.world_to_px(&Vec2D::new(0.0, 0.0));
    println!("world (0, 0) -> px ({}, {})", px_pos[0], px_pos[1]);
    assert!(approx_eq(px_pos[0], 200.0, 1e-1));
    assert!(approx_eq(px_pos[1], 200.0, 1e-1));
}

#[test]
fn test_viewport_transform_window_resize() {
    let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
    let mut transform =
        ViewportTransform::new(world, (400.0, 400.0), Margins::symmetric(50.0, 50.0));

    transform.on_window_resize((800.0, 800.0));

    let viewport_size = transform.viewport_size_px();
    assert!(approx_eq(viewport_size.0, 700.0, 1e-6));
    assert!(approx_eq(viewport_size.1, 700.0, 1e-6));
}

#[test]
fn test_world_to_px_center_at_origin() {
    // World bounds: x=-50 to 50, y=-35 to 35 (centered at origin)
    // Window: 1000x700 with no margins
    // At zoom 1.0, the entire world should fit in the viewport
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let transform = ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    // World origin (0, 0) should map to viewport center (500, 350)
    let px = transform.world_to_px(&Vec2D::new(0.0, 0.0));
    assert!(approx_eq(px[0], 500.0, 1e-3));
    assert!(approx_eq(px[1], 350.0, 1e-3));

    // Top-left world corner (-50, 35) should map to top-left viewport (0, 0)
    let px = transform.world_to_px(&Vec2D::new(-50.0, 35.0));
    assert!(approx_eq(px[0], 0.0, 1e-3));
    assert!(approx_eq(px[1], 0.0, 1e-3));

    // Bottom-right world corner (50, -35) should map to bottom-right viewport (1000, 700)
    let px = transform.world_to_px(&Vec2D::new(50.0, -35.0));
    assert!(approx_eq(px[0], 1000.0, 1e-3));
    assert!(approx_eq(px[1], 700.0, 1e-3));
}

#[test]
fn test_px_to_world_center_at_origin() {
    // Same setup as above
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let transform = ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    // Viewport center (500, 350) should map to world origin (0, 0)
    let w = transform.px_to_world(&[500.0, 350.0]);
    assert!(approx_eq(w.x, 0.0, 1e-3));
    assert!(approx_eq(w.y, 0.0, 1e-3));

    // Top-left viewport (0, 0) should map to top-left world (-50, 35)
    let w = transform.px_to_world(&[0.0, 0.0]);
    assert!(approx_eq(w.x, -50.0, 1e-3));
    assert!(approx_eq(w.y, 35.0, 1e-3));

    // Bottom-right viewport (1000, 700) should map to bottom-right world (50, -35)
    let w = transform.px_to_world(&[1000.0, 700.0]);
    assert!(approx_eq(w.x, 50.0, 1e-3));
    assert!(approx_eq(w.y, -35.0, 1e-3));
}

#[test]
fn test_roundtrip_world_to_px_to_world() {
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let transform = ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    // Test multiple points for roundtrip consistency
    let test_points = [
        Vec2D::new(0.0, 0.0),
        Vec2D::new(-25.0, 17.5),
        Vec2D::new(25.0, -17.5),
        Vec2D::new(-50.0, 35.0),
        Vec2D::new(50.0, -35.0),
    ];

    for orig in &test_points {
        let px = transform.world_to_px(orig);
        let back = transform.px_to_world(&px);
        assert!(
            approx_eq(back.x, orig.x, 1e-3) && approx_eq(back.y, orig.y, 1e-3),
            "Roundtrip failed for ({}, {}): got ({}, {})",
            orig.x,
            orig.y,
            back.x,
            back.y
        );
    }
}

#[test]
fn test_roundtrip_px_to_world_to_px() {
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let transform = ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    // Test multiple pixel positions for roundtrip consistency
    let test_points = [
        [500.0, 350.0],
        [0.0, 0.0],
        [1000.0, 700.0],
        [250.0, 175.0],
        [750.0, 525.0],
    ];

    for orig in &test_points {
        let w = transform.px_to_world(orig);
        let back = transform.world_to_px(&w);
        assert!(
            approx_eq(back[0], orig[0], 1e-3) && approx_eq(back[1], orig[1], 1e-3),
            "Roundtrip failed for ({}, {}): got ({}, {})",
            orig[0],
            orig[1],
            back[0],
            back[1]
        );
    }
}

#[test]
fn test_world_to_px_with_margins() {
    // World: 100x75, Window: 1000x700, Margins: 100x50
    // Viewport region: 800x600 (starts at 100, 50), same shape as the world
    let world = Rect::new(-50.0, 37.5, 100.0, -75.0);
    let transform = ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(100.0, 50.0));

    // World origin should map to viewport center, which is at (100+400, 50+300) = (500, 350)
    let px = transform.world_to_px(&Vec2D::new(0.0, 0.0));
    assert!(approx_eq(px[0], 500.0, 1e-3));
    assert!(approx_eq(px[1], 350.0, 1e-3));

    // Top-left world corner should map to top-left of viewport region (100, 50)
    let px = transform.world_to_px(&Vec2D::new(-50.0, 37.5));
    assert!(approx_eq(px[0], 100.0, 1e-3));
    assert!(approx_eq(px[1], 50.0, 1e-3));

    // Bottom-right world corner should map to bottom-right of viewport region (900, 650)
    let px = transform.world_to_px(&Vec2D::new(50.0, -37.5));
    assert!(approx_eq(px[0], 900.0, 1e-3));
    assert!(approx_eq(px[1], 650.0, 1e-3));
    assert_eq!(transform.inset_margins(), Margins::symmetric(100.0, 50.0));
}

#[test]
fn test_world_of_another_shape_is_letterboxed_not_stretched() {
    // World 100x70 (wider than the 800x600 drawing area): bars above and
    // below, 8 px per world unit on both axes.
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let transform = ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(100.0, 50.0));

    let vp = transform.viewport_px();
    assert!(approx_eq(vp.x, 100.0, 1e-3) && approx_eq(vp.w, 800.0, 1e-3));
    assert!(approx_eq(vp.y, 70.0, 1e-3) && approx_eq(vp.h, 560.0, 1e-3));
    assert_eq!(transform.inset_margins(), Margins::symmetric(100.0, 70.0));

    let top_left = transform.world_to_px(&Vec2D::new(-50.0, 35.0));
    let bottom_right = transform.world_to_px(&Vec2D::new(50.0, -35.0));
    assert!(approx_eq(top_left[0], 100.0, 1e-3) && approx_eq(top_left[1], 70.0, 1e-3));
    assert!(approx_eq(bottom_right[0], 900.0, 1e-3) && approx_eq(bottom_right[1], 630.0, 1e-3));
    let center = transform.world_to_px(&Vec2D::zero());
    assert!(approx_eq(center[0], 500.0, 1e-3) && approx_eq(center[1], 350.0, 1e-3));

    // A taller world gets bars left and right instead.
    let mut transform = transform;
    transform.set_world_bounds(Rect::new(-30.0, 30.0, 60.0, -60.0));
    let vp = transform.viewport_px();
    assert!(approx_eq(vp.x, 200.0, 1e-3) && approx_eq(vp.w, 600.0, 1e-3));
    assert!(approx_eq(vp.y, 50.0, 1e-3) && approx_eq(vp.h, 600.0, 1e-3));
}

#[test]
fn test_zoom_at_center() {
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let mut transform =
        ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    // Zoom 2x at center (0, 0)
    transform.zoom_at_point(&Vec2D::new(0.0, 0.0), 2.0);
    assert!(approx_eq(transform.zoom(), 2.0, 1e-6));

    // World origin should still map to viewport center
    let px = transform.world_to_px(&Vec2D::new(0.0, 0.0));
    assert!(approx_eq(px[0], 500.0, 1e-3));
    assert!(approx_eq(px[1], 350.0, 1e-3));

    // At 2x zoom, only half the world is visible
    // World point at (25, -17.5) should now map to bottom-right (1000, 700)
    let px = transform.world_to_px(&Vec2D::new(25.0, -17.5));
    assert!(approx_eq(px[0], 1000.0, 1e-3));
    assert!(approx_eq(px[1], 700.0, 1e-3));
}

#[test]
fn test_zoom_at_off_center_point() {
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let mut transform =
        ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    // Zoom 2x at point (25, 17.5) - top-right quadrant
    let zoom_point = Vec2D::new(25.0, 17.5);
    transform.zoom_at_point(&zoom_point, 2.0);

    // The zoom point should remain at the same pixel position
    // At zoom 1.0, (25, 17.5) maps to (750, 175)
    let px = transform.world_to_px(&zoom_point);
    assert!(approx_eq(px[0], 750.0, 1.0)); // Allow some tolerance
    assert!(approx_eq(px[1], 175.0, 1.0));
}

#[test]
fn test_pan() {
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let mut transform =
        ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    // At zoom 1.0, pan is constrained to (0, 0) because the entire world is visible
    // We need to zoom in first to allow panning
    transform.zoom_at_point(&Vec2D::zero(), 2.0);

    // Now pan to (10, 5) - should be allowed at zoom 2.0
    transform.pan(Vec2D::new(10.0, 5.0));

    // Verify pan offset (may be clamped slightly by constraints)
    let pan = transform.pan_offset();
    assert!(approx_eq(pan.x, 10.0, 1e-3));
    assert!(approx_eq(pan.y, 5.0, 1e-3));

    // The viewport center (500, 350) should show world point (10, 5)
    let w = transform.px_to_world(&[500.0, 350.0]);
    assert!(approx_eq(w.x, 10.0, 1e-3));
    assert!(approx_eq(w.y, 5.0, 1e-3));

    // And world point (10, 5) should be at viewport center
    let px = transform.world_to_px(&Vec2D::new(10.0, 5.0));
    assert!(approx_eq(px[0], 500.0, 1e-3));
    assert!(approx_eq(px[1], 350.0, 1e-3));
}

#[test]
fn test_visible_world_rect_no_zoom() {
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let transform = ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    let visible = transform.visible_world_rect();
    // At zoom 1.0, entire world should be visible
    assert!(approx_eq(visible.w, 100.0, 1e-3));
    assert!(approx_eq(visible.h.abs(), 70.0, 1e-3));
}

#[test]
fn test_visible_world_rect_with_zoom() {
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let mut transform =
        ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    transform.zoom_at_point(&Vec2D::zero(), 2.0);

    let visible = transform.visible_world_rect();
    // At zoom 2.0, half the world should be visible
    assert!(approx_eq(visible.w, 50.0, 1e-3));
    assert!(approx_eq(visible.h.abs(), 35.0, 1e-3));
}

#[test]
fn test_is_circle_visible() {
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let transform = ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    // Circle at center should be visible
    assert!(transform.is_circle_visible(&Vec2D::zero(), 5.0));

    // Circle at edge should be visible
    assert!(transform.is_circle_visible(&Vec2D::new(45.0, 0.0), 5.0));

    // Circle just outside should be visible (overlaps edge)
    assert!(transform.is_circle_visible(&Vec2D::new(52.0, 0.0), 5.0));

    // Circle far outside should not be visible
    assert!(!transform.is_circle_visible(&Vec2D::new(100.0, 0.0), 5.0));
}

#[test]
fn test_reset() {
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let mut transform =
        ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    // Apply some transforms
    transform.zoom_at_point(&Vec2D::zero(), 3.0);
    transform.pan(Vec2D::new(10.0, 5.0));

    assert!(approx_eq(transform.zoom(), 3.0, 1e-6));

    // Reset
    transform.reset();

    assert!(approx_eq(transform.zoom(), 1.0, 1e-6));
    assert!(approx_eq(transform.pan_offset().x, 0.0, 1e-6));
    assert!(approx_eq(transform.pan_offset().y, 0.0, 1e-6));
}

#[test]
fn test_adjust_pan() {
    let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
    let mut transform =
        ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

    // At zoom 1.0, pan is constrained to (0, 0) - can't pan when entire world is visible
    // Zoom in first to allow panning
    transform.zoom_at_point(&Vec2D::zero(), 4.0);

    // Start at origin (after zoom, pan is still 0)
    assert!(approx_eq(transform.pan_offset().x, 0.0, 1e-6));

    // Adjust pan by (5, 3)
    transform.adjust_pan(&Vec2D::new(5.0, 3.0));

    assert!(approx_eq(transform.pan_offset().x, 5.0, 1e-3));
    assert!(approx_eq(transform.pan_offset().y, 3.0, 1e-3));

    // Adjust again
    transform.adjust_pan(&Vec2D::new(-2.0, 1.0));

    assert!(approx_eq(transform.pan_offset().x, 3.0, 1e-3));
    assert!(approx_eq(transform.pan_offset().y, 4.0, 1e-3));
}

#[test]
fn test_compute_world_bounds_from_window_size() {
    // Window 1000x700 with margins 100x50 -> viewport 800x600
    // At LEN_TO_PX = 0.1, world size = 80x60
    let world = ViewportTransform::compute_world_bounds_from_window_size(
        1000,
        700,
        Margins::symmetric(100.0, 50.0),
    );

    assert!(approx_eq(world.w, 80.0, 1e-3));
    assert!(approx_eq(world.h.abs(), 60.0, 1e-3));
    // Centered at origin
    assert!(approx_eq(world.x, -40.0, 1e-3));
    assert!(approx_eq(world.y, 30.0, 1e-3));
}

#[test]
fn test_zoom_limits() {
    let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
    let mut transform = ViewportTransform::new(world, (400.0, 400.0), Margins::symmetric(0.0, 0.0));

    // Try to zoom out below minimum - should clamp to MIN_ZOOM
    let point = Vec2D::zero();
    transform.zoom_at_point(&point, 0.5);
    assert!(approx_eq(
        transform.zoom(),
        ViewportTransform::MIN_ZOOM,
        1e-6
    ));

    // Zoom in to maximum
    transform.zoom_at_point(&point, 200.0);
    assert!(approx_eq(
        transform.zoom(),
        ViewportTransform::MAX_ZOOM,
        1e-6
    ));

    // Try to zoom beyond maximum - should return false and not change
    let changed = transform.zoom_at_point(&point, 2.0);
    assert!(!changed);
    assert!(approx_eq(
        transform.zoom(),
        ViewportTransform::MAX_ZOOM,
        1e-6
    ));
}

#[test]
fn test_pan_constraints_at_zoom_1() {
    // World bounds: x=-10 to 10, y=-10 to 10 (note: height is negative in rect)
    let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
    let mut transform = ViewportTransform::new(world, (400.0, 400.0), Margins::symmetric(0.0, 0.0));

    // At 1x zoom, the visible area is the same size as the world
    // So pan should be constrained to stay at world center (0, 0)
    transform.pan(Vec2D::new(1000.0, 1000.0));
    assert!(approx_eq(transform.pan_offset().x, 0.0, 1e-6));
    assert!(approx_eq(transform.pan_offset().y, 0.0, 1e-6));

    // Try to pan in negative direction at 1x zoom
    transform.pan(Vec2D::new(-1000.0, -1000.0));
    assert!(approx_eq(transform.pan_offset().x, 0.0, 1e-6));
    assert!(approx_eq(transform.pan_offset().y, 0.0, 1e-6));
}

#[test]
fn test_pan_constraints_at_zoom_2() {
    let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
    let mut transform = ViewportTransform::new(world, (400.0, 400.0), Margins::symmetric(0.0, 0.0));

    // At 2x zoom, the visible area is half the world size
    // So we can pan up to ±5 in each direction
    transform.zoom_at_point(&Vec2D::zero(), 2.0);

    transform.pan(Vec2D::new(1000.0, 1000.0));
    assert!(approx_eq(transform.pan_offset().x, 5.0, 1e-6));
    assert!(approx_eq(transform.pan_offset().y, 5.0, 1e-6));

    transform.pan(Vec2D::new(-1000.0, -1000.0));
    assert!(approx_eq(transform.pan_offset().x, -5.0, 1e-6));
    assert!(approx_eq(transform.pan_offset().y, -5.0, 1e-6));

    // Pan to a position within bounds at 2x zoom should work
    transform.pan(Vec2D::new(3.0, -2.0));
    assert!(approx_eq(transform.pan_offset().x, 3.0, 1e-6));
    assert!(approx_eq(transform.pan_offset().y, -2.0, 1e-6));
}

/// A world derived from its window: 0.1 world units per pixel (`LEN_TO_PX`).
fn window_derived_transform() -> ViewportTransform {
    let margins = Margins::symmetric(0.0, 0.0);
    let world = ViewportTransform::compute_world_bounds_from_window_size(1000, 700, margins);
    ViewportTransform::new(world, (1000.0, 700.0), margins)
}

#[test]
fn test_apply_pan_delta() {
    let transform = window_derived_transform();
    let start_offset = Vec2D::new(0.0, 0.0);

    // Move 100 pixels right and 50 pixels down
    // LEN_TO_PX = 0.1, so 100 pixels = 10 world units
    // Pan direction is inverted, so moving right in pixels moves viewport left in world
    let new_pan = transform.apply_pan_delta(Vec2D::new(100.0, 50.0), start_offset);

    assert!(approx_eq(new_pan.x, -10.0, 1e-5));
    assert!(approx_eq(new_pan.y, 5.0, 1e-5));
}

#[test]
fn test_apply_pan_delta_with_zoom() {
    // At zoom 2.0, pixel movements should result in half the world movement
    let mut transform = window_derived_transform();
    transform.zoom_at_point(&Vec2D::zero(), 2.0);
    let start_offset = Vec2D::new(5.0, 3.0);

    let new_pan = transform.apply_pan_delta(Vec2D::new(100.0, 50.0), start_offset);

    // world_dx = 100 * 0.1 / 2.0 = 5, inverted = -5
    // world_dy = 50 * 0.1 / 2.0 = 2.5, Y flip = +2.5
    assert!(approx_eq(new_pan.x, 0.0, 1e-5));
    assert!(approx_eq(new_pan.y, 5.5, 1e-5));
}

#[test]
fn test_pan_follows_the_cursor_on_a_letterboxed_world() {
    // A 34x40 world (saved in a 500x500 window) in a 1000x2000 window with
    // the default margins: drawn at 840/34 px per unit, not 10.
    let margins = Margins::new(110.0, 50.0, 50.0, 50.0);
    let world = Rect::new(-17.0, 20.0, 34.0, -40.0);
    let transform = ViewportTransform::new(world, (1000.0, 2000.0), margins);
    assert!(approx_eq(transform.world_per_px(), 34.0 / 840.0, 1e-6));

    // Dragging by 84 px moves the content by exactly 84 px on screen.
    let new_pan = transform.apply_pan_delta(Vec2D::new(84.0, 0.0), Vec2D::zero());
    let a = transform.world_to_px(&Vec2D::zero());
    let b = transform.world_to_px(&Vec2D::new(-new_pan.x, 0.0));
    assert!(approx_eq(b[0] - a[0], 84.0, 1e-3));
}
