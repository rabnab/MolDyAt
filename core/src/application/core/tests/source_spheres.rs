use crate::application::MainContent;
use crate::objects2d::{OperatorCollection, OperatorState, Rect, Vec2D};

fn make_content() -> MainContent {
    MainContent::new(Rect::new(-20.0, 20.0, 40.0, -40.0)).unwrap()
}

// ── Placement and lifecycle ─────────────────────────────────────────

#[test]
fn add_source_sphere_increases_count() {
    let mut mc = make_content();
    mc.add_source_sphere(Vec2D::new(0.0, 0.0));
    mc.add_source_sphere(Vec2D::new(5.0, 5.0));
    assert_eq!(mc.source_spheres.spheres().len(), 2);
}

#[test]
fn added_sphere_starts_on() {
    let mut mc = make_content();
    mc.add_source_sphere(Vec2D::new(0.0, 0.0));
    let sphere = &mc.source_spheres.spheres()[0];
    assert_eq!(sphere.state, OperatorState::On);
    assert!(sphere.is_emitting());
}

#[test]
fn ids_are_sequential() {
    let mut mc = make_content();
    mc.add_source_sphere(Vec2D::new(0.0, 0.0));
    mc.add_source_sphere(Vec2D::new(1.0, 0.0));
    let ids: Vec<usize> = mc.source_spheres.spheres().iter().map(|s| s.id).collect();
    assert_eq!(ids, vec![0, 1]);
}

// ── Toggle ──────────────────────────────────────────────────────────

#[test]
fn toggle_turns_sphere_off() {
    let mut mc = make_content();
    mc.add_source_sphere(Vec2D::new(0.0, 0.0));
    let id = mc.source_spheres.spheres()[0].id;

    mc.toggle_source_sphere(id);
    let sphere = mc.source_spheres.find(id).unwrap();
    assert_eq!(sphere.state, OperatorState::Off);
    assert!(!sphere.is_emitting());
}

#[test]
fn double_toggle_restores_on() {
    let mut mc = make_content();
    mc.add_source_sphere(Vec2D::new(0.0, 0.0));
    let id = mc.source_spheres.spheres()[0].id;

    mc.toggle_source_sphere(id);
    mc.toggle_source_sphere(id);
    assert_eq!(mc.source_spheres.find(id).unwrap().state, OperatorState::On);
}

#[test]
fn toggle_unknown_id_is_noop() {
    let mut mc = make_content();
    mc.add_source_sphere(Vec2D::new(0.0, 0.0));
    // Toggling a non-existent ID should not panic.
    mc.toggle_source_sphere(999);
    assert_eq!(mc.source_spheres.spheres().len(), 1);
}

// ── Clear lifecycle ─────────────────────────────────────────────────

#[test]
fn clear_removes_all_spheres_and_resets_ids() {
    let mut mc = make_content();
    mc.add_source_sphere(Vec2D::new(0.0, 0.0));
    mc.add_source_sphere(Vec2D::new(1.0, 0.0));
    mc.source_spheres.clear();
    assert!(mc.source_spheres.spheres().is_empty());

    // After clear, IDs restart from 0.
    mc.add_source_sphere(Vec2D::new(2.0, 0.0));
    assert_eq!(mc.source_spheres.spheres()[0].id, 0);
}

// ── Rescale ─────────────────────────────────────────────────────────

#[test]
fn rescale_remaps_center_proportionally() {
    use crate::objects2d::OperatorCollection;
    let mut mc = make_content();
    // Place sphere at center of old rect.
    mc.add_source_sphere(Vec2D::new(0.0, 0.0));

    let old_rect = Rect::new(-20.0, 20.0, 40.0, -40.0);
    // Double the rect.
    let new_rect = Rect::new(-40.0, 40.0, 80.0, -80.0);
    mc.source_spheres.rescale(&old_rect, &new_rect);

    let sphere = &mc.source_spheres.spheres()[0];
    // Center (0,0) was at fraction (0.5,0.5) in old rect.
    // In new rect: x = -40 + 0.5*80 = 0, y = 40 + 0.5*(-80) = 0.
    assert!((sphere.center.x - 0.0).abs() < 1e-4);
    assert!((sphere.center.y - 0.0).abs() < 1e-4);
}

#[test]
fn rescale_degenerate_rect_is_noop() {
    use crate::objects2d::OperatorCollection;
    let mut mc = make_content();
    mc.add_source_sphere(Vec2D::new(5.0, 3.0));

    let degenerate = Rect::new(0.0, 0.0, 0.0, 0.0); // zero area
    let new_rect = Rect::new(-40.0, 40.0, 80.0, -80.0);
    mc.source_spheres.rescale(&degenerate, &new_rect);

    let sphere = &mc.source_spheres.spheres()[0];
    // Position should be unchanged because old_rect is degenerate.
    assert!((sphere.center.x - 5.0).abs() < 1e-4);
    assert!((sphere.center.y - 3.0).abs() < 1e-4);
}
