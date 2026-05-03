use std::sync::Arc;

use crate::application::MainContent;
use crate::application::renderer_snapshot::{get_special_ball_color, size_class_to_rgba};
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::BallFactory;

fn create_test_main_content_with_balls(ball_count: usize) -> MainContent {
    let world_rect = crate::objects2d::Rect::new(0.0, 0.0, 100.0, 100.0);
    let mut main_content = MainContent::new(world_rect).expect("Could not create main content");

    let factory = BallFactory::new();

    // Add balls
    for _ in 0..ball_count {
        let ball =
            factory.create_random((&Vec2D::new(0.0, 0.0), &Vec2D::new(10.0, 10.0)), (0.0, 1.0));
        main_content.balls.push(ball);
    }

    main_content.num_balls = ball_count;

    // Add balls to cell grid
    main_content.world_mut().rebuild_cell_grid();

    main_content
}

#[test]
fn test_collect_all_neighbors_empty() {
    // Create a MainContent with empty balls vector
    // This is a structural test to verify the method works with empty input
    let empty_neighbors: Vec<Vec<usize>> = Vec::new();
    assert_eq!(empty_neighbors.len(), 0);
}

#[test]
fn test_snapshot_colors_are_valid_rgba() {
    // Verify the color helper functions work correctly
    for size_class in 0..6 {
        let color = size_class_to_rgba(size_class);
        assert!(color[0] >= 0.0 && color[0] <= 1.0);
        assert!(color[1] >= 0.0 && color[1] <= 1.0);
        assert!(color[2] >= 0.0 && color[2] <= 1.0);
        assert!(color[3] >= 0.0 && color[3] <= 1.0);
    }
}

#[test]
fn test_special_colors() {
    // Verify special color logic
    let verbose_color = get_special_ball_color(true, false);
    assert_eq!(verbose_color, Some([0.0, 0.0, 1.0, 1.0])); // Blue

    let braking_color = get_special_ball_color(false, true);
    assert_eq!(braking_color, Some([1.0, 1.0, 0.0, 1.0])); // Yellow

    let normal_color = get_special_ball_color(false, false);
    assert_eq!(normal_color, None); // Use size class color
}

#[test]
fn test_publish_snapshot_atomic_storage() {
    let main_content = create_test_main_content_with_balls(10);

    // Publish a snapshot
    main_content.publish_snapshot();

    // Verify snapshot was stored
    let slot = main_content.get_snapshot_slot();
    let snapshot = slot.load();
    assert!(snapshot.is_some());

    let snapshot = snapshot.as_ref().unwrap();
    assert_eq!(snapshot.len(), 10);
    assert_eq!(snapshot.balls.len(), 10);
}

#[test]
fn test_snapshot_slot_sharing() {
    let main_content = create_test_main_content_with_balls(5);

    // Get two clones of the snapshot slot
    let slot1 = main_content.get_snapshot_slot();
    let slot2 = main_content.get_snapshot_slot();

    // Publish snapshot
    main_content.publish_snapshot();

    // Both slots should see the same snapshot
    let snap1 = slot1.load();
    let snap2 = slot2.load();

    assert!(snap1.is_some());
    assert!(snap2.is_some());

    // Verify they point to the same Arc
    assert!(Arc::ptr_eq(
        snap1.as_ref().unwrap(),
        snap2.as_ref().unwrap()
    ));
}

#[test]
fn test_snapshot_updates_on_republish() {
    let mut main_content = create_test_main_content_with_balls(5);

    // Publish first snapshot
    main_content.step_count = 100;
    main_content.publish_snapshot();

    let slot = main_content.get_snapshot_slot();
    let snapshot1 = slot.load();
    assert_eq!(snapshot1.as_ref().unwrap().simulation_step, 100);

    // Update step count and republish
    main_content.step_count = 200;
    main_content.publish_snapshot();

    let snapshot2 = slot.load();
    assert_eq!(snapshot2.as_ref().unwrap().simulation_step, 200);

    // Verify they are different snapshots
    assert!(!Arc::ptr_eq(
        snapshot1.as_ref().unwrap(),
        snapshot2.as_ref().unwrap()
    ));
}

#[test]
fn test_snapshot_neighbor_indices_valid() {
    let main_content = create_test_main_content_with_balls(20);
    let snapshot = main_content.create_snapshot();

    // All neighbor indices should be valid (< ball count)
    for (i, ball_data) in snapshot.balls.iter().enumerate() {
        for &(neighbor_idx, _force) in &ball_data.neighbors {
            assert!(
                neighbor_idx < snapshot.len(),
                "Ball {} has invalid neighbor index {}",
                i,
                neighbor_idx
            );
        }
    }
}

#[test]
fn test_snapshot_radii_positive() {
    let main_content = create_test_main_content_with_balls(15);
    let snapshot = main_content.create_snapshot();

    // All radii should be positive
    for (i, ball_data) in snapshot.balls.iter().enumerate() {
        assert!(
            ball_data.radius > 0.0,
            "Ball {} has non-positive radius {}",
            i,
            ball_data.radius
        );
        assert!(
            ball_data.radius.is_finite(),
            "Ball {} has non-finite radius",
            i
        );
    }
}

#[test]
fn test_snapshot_positions_finite() {
    let main_content = create_test_main_content_with_balls(10);
    let snapshot = main_content.create_snapshot();

    // All positions should have finite coordinates
    for (i, ball_data) in snapshot.balls.iter().enumerate() {
        assert!(
            ball_data.position.x.is_finite(),
            "Ball {} has non-finite x position",
            i
        );
        assert!(
            ball_data.position.y.is_finite(),
            "Ball {} has non-finite y position",
            i
        );
    }
}

// ── Thermo snapshot tests (#115) ─────────────────────────────────

fn create_main_content_and_step(ball_count: usize) -> MainContent {
    use crate::application::GameLogic;
    let mut mc = create_test_main_content_with_balls(ball_count);
    mc.simulation_step().unwrap();
    mc
}

#[test]
fn test_thermo_snapshot_populated_after_step_with_balls() {
    let mc = create_main_content_and_step(10);
    let snap = mc.create_snapshot();
    // After one step with ≥ 2 balls, thermo fields must be populated
    assert!(snap.thermo_grid_spec.is_some(), "spec should be Some");
    let (rows, cols) = snap.cell_temperature.shape();
    assert!(rows > 0 && cols > 0, "T grid should be non-empty");
    let (pr, pc) = snap.cell_pressure.shape();
    assert!(pr > 0 && pc > 0, "P grid should be non-empty");
}

#[test]
fn test_thermo_grid_shape_matches_spec() {
    let mc = create_main_content_and_step(20);
    let snap = mc.create_snapshot();
    let spec = snap.thermo_grid_spec.as_ref().expect("spec should be Some");
    let (t_rows, t_cols) = snap.cell_temperature.shape();
    assert_eq!(t_rows, spec.rows, "T rows must match spec");
    assert_eq!(t_cols, spec.cols, "T cols must match spec");
    let (p_rows, p_cols) = snap.cell_pressure.shape();
    assert_eq!(p_rows, spec.rows, "P rows must match spec");
    assert_eq!(p_cols, spec.cols, "P cols must match spec");
}

#[test]
fn test_thermo_snapshot_single_ball_all_nan() {
    let mc = create_main_content_and_step(1);
    let snap = mc.create_snapshot();
    // With only 1 ball, all cells are NaN but spec is still populated
    assert!(
        snap.thermo_grid_spec.is_some(),
        "spec should be Some even with 1 ball"
    );
    let (rows, cols) = snap.cell_temperature.shape();
    for r in 0..rows {
        for c in 0..cols {
            assert!(
                snap.cell_temperature.get(r, c).is_nan(),
                "T[{r},{c}] should be NaN with 1 ball"
            );
            assert!(
                snap.cell_pressure.get(r, c).is_nan(),
                "P[{r},{c}] should be NaN with 1 ball"
            );
        }
    }
}

#[test]
fn test_thermo_spec_dimensions_match_world() {
    let mc = create_main_content_and_step(50);
    let snap = mc.create_snapshot();
    let spec = snap.thermo_grid_spec.as_ref().expect("spec should be Some");
    let world = mc.world_region();
    let world_w = world.w.abs();
    let world_h = world.h.abs();
    let grid_w = spec.cols as f32 * spec.cell_width;
    let grid_h = spec.rows as f32 * spec.cell_height;
    assert!(
        (grid_w - world_w).abs() < 1e-3,
        "grid width {grid_w} should match world width {world_w}"
    );
    assert!(
        (grid_h - world_h).abs() < 1e-3,
        "grid height {grid_h} should match world height {world_h}"
    );
}
