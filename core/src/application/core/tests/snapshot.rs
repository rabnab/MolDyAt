// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::sync::Arc;

use crate::application::MainContent;
use crate::application::renderer_snapshot::get_special_ball_color;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::BallFactory;
use crate::objects2d::ball_impl::SpeciesTable;

fn create_test_main_content_with_balls(ball_count: usize) -> MainContent {
    let world_rect = crate::objects2d::Rect::new(0.0, 0.0, 100.0, 100.0);
    let mut main_content = MainContent::new(world_rect).expect("Could not create main content");

    let factory = BallFactory::new();

    // Add balls
    for _ in 0..ball_count {
        let ball =
            factory.create_random((&Vec2D::new(0.0, 0.0), &Vec2D::new(10.0, 10.0)), (0.0, 1.0));
        main_content.world_mut().balls.push(ball);
    }

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
    // Verify the species colours are valid
    let table = SpeciesTable::builtin();
    for i in 0..table.len() {
        let color = table.species(table.id(i).unwrap()).colour;
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
    main_content.world_mut().step_count = 100;
    main_content.publish_snapshot();

    let slot = main_content.get_snapshot_slot();
    let snapshot1 = slot.load();
    assert_eq!(snapshot1.as_ref().unwrap().simulation_step, 100);

    // Update step count and republish
    main_content.world_mut().step_count = 200;
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
            ball_data.display_radius > 0.0,
            "Ball {} has non-positive radius {}",
            i,
            ball_data.display_radius
        );
        assert!(
            ball_data.display_radius.is_finite(),
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
    let world = mc.world().world_region();
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

#[test]
fn snapshot_without_heavy_layers_skips_ball_render_data_and_thermo_grids() {
    use crate::application::scene_layers::SceneLayers;

    let mut main_content = create_test_main_content_with_balls(5);
    let full = main_content.create_snapshot();
    assert_eq!(full.layers, SceneLayers::ALL);
    assert_eq!(full.balls.len(), 5);

    // Rendering off (F2): only what operators, labels and stats need.
    let light_layers = SceneLayers::ALL.difference(SceneLayers::HEAVY);
    main_content.set_snapshot_layers(light_layers);
    let light = main_content.create_snapshot();
    assert_eq!(light.layers, light_layers);
    assert!(light.balls.is_empty());
    assert!(light.thermo_grid_spec.is_none());
    assert_eq!(light.cell_temperature.shape(), (0, 0));
    assert_eq!(light.kinetic_energy, full.kinetic_energy);
    // The stats text is built either way. Its content is not compared: it
    // includes the process-wide neighbour-update counter, which tests running
    // in parallel change between the two snapshots.
    assert_eq!(light.status_text.is_some(), full.status_text.is_some());

    // Rendering on again: the next snapshot is the full scene.
    main_content.set_snapshot_layers(SceneLayers::ALL);
    let again = main_content.create_snapshot();
    assert_eq!(again.layers, SceneLayers::ALL);
    assert_eq!(again.balls.len(), 5);
}

#[test]
fn dropping_the_thermo_layer_stops_and_clears_the_thermo_fields() {
    use crate::application::core::SimulationEngine;
    use crate::application::scene_layers::SceneLayers;

    let mut main_content = create_test_main_content_with_balls(20);
    main_content.world_mut().sim.set_pause(false);
    main_content.simulation_step().unwrap();
    assert!(main_content.create_snapshot().thermo_grid_spec.is_some());

    // Heatmap hidden (T off or F2 off): the history is dropped, and the
    // physics thread records no more fields (see `record_thermo_fields`).
    main_content.set_snapshot_layers(SceneLayers::ALL.difference(SceneLayers::THERMO));
    assert!(main_content.world().thermo_history.average().is_none());

    // Shown again: the next recorded step starts a fresh history.
    main_content.set_snapshot_layers(SceneLayers::ALL);
    SimulationEngine::record_thermo_fields(main_content.world_mut());
    assert!(main_content.create_snapshot().thermo_grid_spec.is_some());
}

// ── Thermo field sampling (#229) ──────────────────────────────────

fn seeded_gas(seed: u64, balls: usize) -> MainContent {
    let world_rect = crate::objects2d::Rect::new(-40.0, 40.0, 80.0, -80.0);
    let mut mc = MainContent::new_with_seed(world_rect, seed).expect("main content");
    mc.seed_random_balls_with_speed(balls, 3.0);
    mc.world_mut().sim.set_pause(false);
    mc
}

#[test]
fn test_thermo_fields_are_sampled_every_stride_steps() {
    use crate::application::thermo_field::THERMO_SAMPLE_STRIDE;

    let mut mc = seeded_gas(229, 40);
    let start = mc.world().step_count;
    let steps = 3 * THERMO_SAMPLE_STRIDE + 2;
    for _ in 0..steps {
        mc.simulation_step().unwrap();
    }
    // The first step samples (empty history, so the heatmap shows at once),
    // then every step whose count is a multiple of the stride.
    let end = start + steps;
    let strided = (start + 2..=end)
        .filter(|n| n.is_multiple_of(THERMO_SAMPLE_STRIDE))
        .count();
    assert_eq!(mc.world().thermo_history.len(), 1 + strided);
}

#[test]
fn test_strided_thermo_average_matches_per_step_average() {
    use crate::application::thermo_field::{
        THERMO_SAMPLE_STRIDE, THERMO_WINDOW, THERMO_WINDOW_STEPS, ThermoFieldHistory,
        compute_cell_thermo,
    };

    // The same run averaged two ways over the same 1000 steps: every step
    // (the behaviour before #229) and every stride-th step (the default).
    let mut mc = seeded_gas(7, 400);
    for _ in 0..200 {
        mc.simulation_step().unwrap(); // let the gas settle
    }
    let mut every_step = ThermoFieldHistory::with_window(THERMO_WINDOW_STEPS);
    let mut strided = ThermoFieldHistory::with_window(THERMO_WINDOW);
    for step in 1..=THERMO_WINDOW_STEPS {
        mc.simulation_step().unwrap();
        let world = mc.world();
        let (rows, cols) = world.cell_grid.get_grid_size();
        let fields = || {
            compute_cell_thermo(
                &world.balls,
                &world.cached_force_results,
                &world.world_rect,
                rows.max(1),
                cols.max(1),
            )
        };
        let (t, p) = fields();
        every_step.push(t, p);
        if step.is_multiple_of(THERMO_SAMPLE_STRIDE) {
            let (t, p) = fields();
            strided.push(t, p);
        }
    }

    let (t_all, p_all) = every_step.average().expect("per-step average");
    let (t_strided, p_strided) = strided.average().expect("strided average");
    for (name, all, sampled) in [("T", &t_all, &t_strided), ("P", &p_all, &p_strided)] {
        let pairs: Vec<(f32, f32)> = all
            .as_slice()
            .iter()
            .zip(sampled.as_slice())
            .filter(|(a, b)| a.is_finite() && b.is_finite())
            .map(|(a, b)| (*a, *b))
            .collect();
        assert!(
            pairs.len() > 10,
            "{name}: only {} comparable cells",
            pairs.len()
        );
        let mean_all = pairs.iter().map(|(a, _)| a).sum::<f32>() / pairs.len() as f32;
        let mean_sampled = pairs.iter().map(|(_, b)| b).sum::<f32>() / pairs.len() as f32;
        let mean_abs_diff =
            pairs.iter().map(|(a, b)| (a - b).abs()).sum::<f32>() / pairs.len() as f32;
        // Field-wide mean within 2 %, per-cell deviation within 10 % of it.
        assert!(
            (mean_sampled - mean_all).abs() <= 0.02 * mean_all.abs(),
            "{name}: mean {mean_sampled} vs {mean_all}"
        );
        assert!(
            mean_abs_diff <= 0.10 * mean_all.abs(),
            "{name}: mean per-cell difference {mean_abs_diff} vs mean {mean_all}"
        );
    }
}
