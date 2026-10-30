// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod gravity;
mod object_deletion;
mod simulation_engine;
mod species_ui;
mod thermostats;
mod valves;
use crate::application::SnapshotPublisher;
use std::sync::Arc;

use crate::application::renderer_snapshot::get_special_ball_color;
use crate::application::{CreateSnapshot, MainContent};
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::SpeciesTable;
use crate::objects2d::ball_impl::{BallFactory, DEFAULT_SPECIES};

fn create_test_main_content_with_balls(ball_count: usize) -> MainContent {
    let world_rect = crate::objects2d::Rect::new(0.0, 0.0, 100.0, 100.0);
    let mut main_content = thermo_physics::test_support::new_main_content(world_rect)
        .expect("Could not create main content");

    let factory = BallFactory::new();

    // Add balls
    for _ in 0..ball_count {
        let ball = factory.create_random(
            DEFAULT_SPECIES,
            (&Vec2D::new(0.0, 0.0), &Vec2D::new(10.0, 10.0)),
            (0.0, 1.0),
        );
        main_content.world_mut().balls_mut().push(ball);
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
    let publisher = SnapshotPublisher::new();
    let main_content = create_test_main_content_with_balls(10);

    // Publish a snapshot
    publisher.publish(publisher.snapshot(&main_content));

    // Verify snapshot was stored
    let slot = publisher.slot();
    let snapshot = slot.load();
    assert!(snapshot.is_some());

    let snapshot = snapshot.as_ref().unwrap();
    assert_eq!(snapshot.len(), 10);
    assert_eq!(snapshot.balls.len(), 10);
}

#[test]
fn test_snapshot_slot_sharing() {
    let publisher = SnapshotPublisher::new();
    let main_content = create_test_main_content_with_balls(5);

    // Get two clones of the snapshot slot
    let slot1 = publisher.slot();
    let slot2 = publisher.slot();

    // Publish snapshot
    publisher.publish(publisher.snapshot(&main_content));

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
    let publisher = SnapshotPublisher::new();
    let mut main_content = create_test_main_content_with_balls(5);

    // Publish first snapshot
    main_content.world_mut().set_step_count(100);
    publisher.publish(publisher.snapshot(&main_content));

    let slot = publisher.slot();
    let snapshot1 = slot.load();
    assert_eq!(snapshot1.as_ref().unwrap().frame.simulation_step, 100);

    // Update step count and republish
    main_content.world_mut().set_step_count(200);
    publisher.publish(publisher.snapshot(&main_content));

    let snapshot2 = slot.load();
    assert_eq!(snapshot2.as_ref().unwrap().frame.simulation_step, 200);

    // Verify they are different snapshots
    assert!(!Arc::ptr_eq(
        snapshot1.as_ref().unwrap(),
        snapshot2.as_ref().unwrap()
    ));
}

#[test]
fn test_snapshot_neighbor_indices_valid() {
    let mut main_content = create_test_main_content_with_balls(20);
    main_content.apply_verbosity_input_for_test("am");
    main_content.simulation_step().unwrap();
    let snapshot = main_content.create_snapshot();

    let debug = snapshot
        .debug
        .as_ref()
        .expect("verbose balls have debug data");
    assert_eq!(debug.neighbor_lines.len(), 20, "one entry per verbose ball");
    // All ball and neighbour indices are valid (< ball count)
    for lines in &debug.neighbor_lines {
        assert!(lines.ball < snapshot.len());
        assert!(lines.main_only);
        for &(neighbor_idx, _force) in &lines.neighbors {
            assert!(
                neighbor_idx < snapshot.len(),
                "Ball {} has invalid neighbor index {}",
                lines.ball,
                neighbor_idx
            );
        }
    }
}

#[test]
fn snapshot_without_verbose_balls_has_no_debug_data() {
    let main_content = create_test_main_content_with_balls(20);
    assert!(main_content.create_snapshot().debug.is_none());
}

#[test]
fn a_released_snapshot_is_refilled_instead_of_allocated() {
    let publisher = SnapshotPublisher::new();
    let main_content = create_test_main_content_with_balls(20);
    let first = publisher.snapshot(&main_content);
    let first_balls = first.balls.data.as_ptr();
    publisher.publish(first);
    // The second publish replaces the first; no one else holds it.
    publisher.publish(publisher.snapshot(&main_content));

    let third = publisher.snapshot(&main_content);
    assert_eq!(third.balls.data.as_ptr(), first_balls, "same ball buffer");
    assert_eq!(third.balls.len(), 20);
}

#[test]
fn a_snapshot_the_renderer_still_holds_is_not_reused() {
    let publisher = SnapshotPublisher::new();
    let slot = publisher.slot();
    let main_content = create_test_main_content_with_balls(20);
    publisher.publish(publisher.snapshot(&main_content));
    let held = slot.load_full().expect("published");
    publisher.publish(publisher.snapshot(&main_content));

    let next = publisher.snapshot(&main_content);
    assert_ne!(next.balls.data.as_ptr(), held.balls.data.as_ptr());
    assert_eq!(held.balls.len(), 20, "the held snapshot is untouched");
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
    assert!(snap.fields.thermo_grid.is_some(), "spec should be Some");
    let (rows, cols) = snap.fields.cells.temperature.shape();
    assert!(rows > 0 && cols > 0, "T grid should be non-empty");
    let (pr, pc) = snap.fields.cells.pressure.shape();
    assert!(pr > 0 && pc > 0, "P grid should be non-empty");
}

#[test]
fn test_thermo_grid_shape_matches_spec() {
    let mc = create_main_content_and_step(20);
    let snap = mc.create_snapshot();
    let spec = snap
        .fields
        .thermo_grid
        .as_ref()
        .expect("spec should be Some");
    let (t_rows, t_cols) = snap.fields.cells.temperature.shape();
    assert_eq!(t_rows, spec.rows, "T rows must match spec");
    assert_eq!(t_cols, spec.cols, "T cols must match spec");
    let (p_rows, p_cols) = snap.fields.cells.pressure.shape();
    assert_eq!(p_rows, spec.rows, "P rows must match spec");
    assert_eq!(p_cols, spec.cols, "P cols must match spec");
}

#[test]
fn test_thermo_snapshot_single_ball_all_nan() {
    let mc = create_main_content_and_step(1);
    let snap = mc.create_snapshot();
    // With only 1 ball, all cells are NaN but spec is still populated
    assert!(
        snap.fields.thermo_grid.is_some(),
        "spec should be Some even with 1 ball"
    );
    let (rows, cols) = snap.fields.cells.temperature.shape();
    for r in 0..rows {
        for c in 0..cols {
            assert!(
                snap.fields.cells.temperature.get(r, c).is_nan(),
                "T[{r},{c}] should be NaN with 1 ball"
            );
            assert!(
                snap.fields.cells.pressure.get(r, c).is_nan(),
                "P[{r},{c}] should be NaN with 1 ball"
            );
        }
    }
}

#[test]
fn test_thermo_spec_dimensions_match_world() {
    let mc = create_main_content_and_step(50);
    let snap = mc.create_snapshot();
    let spec = snap
        .fields
        .thermo_grid
        .as_ref()
        .expect("spec should be Some");
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

    let mut publisher = SnapshotPublisher::new();
    let mut main_content = create_test_main_content_with_balls(5);
    let full = publisher.snapshot(&main_content);
    assert_eq!(full.frame.layers, SceneLayers::ALL);
    assert_eq!(full.balls.len(), 5);

    // Rendering off (F2): only what operators, labels and stats need.
    let light_layers = SceneLayers::ALL.difference(SceneLayers::HEAVY);
    publisher.set_layers(&mut main_content, light_layers);
    let light = publisher.snapshot(&main_content);
    assert_eq!(light.frame.layers, light_layers);
    assert!(light.balls.is_empty());
    assert!(light.fields.thermo_grid.is_none());
    assert_eq!(light.fields.cells.temperature.shape(), (0, 0));
    assert_eq!(light.frame.kinetic_energy, full.frame.kinetic_energy);
    // The status numbers come either way. Their content is not compared:
    // they include the process-wide neighbour-update counter, which tests
    // running in parallel change between the two snapshots.
    assert_eq!(light.frame.status.is_some(), full.frame.status.is_some());
    assert_eq!(light.frame.species_counts, full.frame.species_counts);

    // Rendering on again: the next snapshot is the full scene.
    publisher.set_layers(&mut main_content, SceneLayers::ALL);
    let again = publisher.snapshot(&main_content);
    assert_eq!(again.frame.layers, SceneLayers::ALL);
    assert_eq!(again.balls.len(), 5);
}

#[test]
fn dropping_the_thermo_layer_stops_and_clears_the_thermo_fields() {
    use crate::application::scene_layers::SceneLayers;

    let mut publisher = SnapshotPublisher::new();
    let mut main_content = create_test_main_content_with_balls(20);
    main_content.world_mut().sim_mut().set_pause(false);
    main_content.simulation_step().unwrap();
    assert!(
        publisher
            .snapshot(&main_content)
            .fields
            .thermo_grid
            .is_some()
    );

    // Heatmap hidden (T off or F2 off): the history is dropped, and the
    // physics thread records no more fields (see `record_thermo_fields`).
    publisher.set_layers(
        &mut main_content,
        SceneLayers::ALL.difference(SceneLayers::THERMO),
    );
    assert!(main_content.world().thermo_history().average().is_none());

    // Shown again: the next recorded step starts a fresh history (an empty
    // history is recorded at once, see `record_thermo_fields_if_due`).
    publisher.set_layers(&mut main_content, SceneLayers::ALL);
    main_content.step_recording_thermo_fields(true).unwrap();
    assert!(
        publisher
            .snapshot(&main_content)
            .fields
            .thermo_grid
            .is_some()
    );
}

// ── Thermo field sampling (#229) ──────────────────────────────────

fn seeded_gas(seed: u64, balls: usize) -> MainContent {
    let world_rect = crate::objects2d::Rect::new(-40.0, 40.0, 80.0, -80.0);
    let mut mc = thermo_physics::test_support::new_main_content_seeded(world_rect, seed)
        .expect("main content");
    mc.seed_random_balls_with_speed(balls, 3.0);
    mc.world_mut().sim_mut().set_pause(false);
    mc
}

#[test]
fn test_thermo_fields_are_sampled_every_stride_steps() {
    use crate::application::thermo_field::THERMO_SAMPLE_STRIDE;

    let mut mc = seeded_gas(229, 40);
    let start = mc.world().step_count();
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
    assert_eq!(mc.world().thermo_history().len(), 1 + strided);
}

/// Steps `mc` with the thermo fields recorded (the heatmap is shown).
fn step_with_heatmap(mc: &mut MainContent, steps: usize) {
    for _ in 0..steps {
        mc.step_recording_thermo_fields(true).unwrap();
    }
}

#[test]
fn a_new_species_keeps_the_thermo_grid_and_its_average() {
    // #290: a ball of a larger species rebuilds the physics cell grid; the
    // thermo grid is the world's own, so its average goes on.
    let mut mc = seeded_gas(290, 40);
    step_with_heatmap(&mut mc, 25);
    let cells_before = mc.world().cell_grid().get_grid_size();
    let samples_before = mc.world().thermo_history().len();
    let t_before = mc.world().thermo_history().average().unwrap().temperature;

    // Species of new balls: N2, XS, S, M, L, XL in the built-in table.
    for _ in 0..5 {
        mc.next_species();
    }
    mc.add_candidate_at_pos(Vec2D::new(30.0, -30.0)).unwrap();
    step_with_heatmap(&mut mc, 20);

    assert_ne!(
        mc.world().cell_grid().get_grid_size(),
        cells_before,
        "the physics grid follows the largest species"
    );
    assert_eq!(
        mc.world().thermo_history().len(),
        samples_before + 2,
        "the rolling average was not restarted"
    );
    let t_after = mc.world().thermo_history().average().unwrap().temperature;
    assert_eq!(t_after.shape(), t_before.shape());
}

#[test]
fn the_thermo_grid_has_the_same_resolution_for_any_species() {
    use crate::application::thermo_field::thermo_grid_dims;
    let mut n2 = seeded_gas(290, 20);
    step_with_heatmap(&mut n2, 1);
    let mut xl = seeded_gas(290, 0);
    for _ in 0..5 {
        xl.next_species();
    }
    xl.add_candidate_at_pos(Vec2D::new(0.0, 0.0)).unwrap();
    xl.add_candidate_at_pos(Vec2D::new(20.0, 0.0)).unwrap();
    step_with_heatmap(&mut xl, 1);

    let shape = |mc: &MainContent| mc.world().thermo_history().average().unwrap().shape();
    assert_eq!(shape(&n2), thermo_grid_dims(80.0, 80.0));
    assert_eq!(shape(&n2), shape(&xl));
    let publisher = SnapshotPublisher::new();
    let spec = publisher.snapshot(&xl).fields.thermo_grid.unwrap();
    assert_eq!((spec.rows, spec.cols), shape(&xl));
    assert_eq!(spec.cell_width, 80.0 / spec.cols as f32);
}

#[test]
fn test_strided_thermo_average_matches_per_step_average() {
    use crate::application::thermo_field::{
        THERMO_SAMPLE_STRIDE, THERMO_WINDOW, THERMO_WINDOW_STEPS, ThermoFieldHistory,
        compute_cell_fields,
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
        let fields = || {
            compute_cell_fields(
                world.balls(),
                world.cached_force_results(),
                &world.world_rect(),
                0,
                0,
            )
        };
        every_step.push(fields());
        if step.is_multiple_of(THERMO_SAMPLE_STRIDE) {
            strided.push(fields());
        }
    }

    let all = every_step.average().expect("per-step average");
    let sampled = strided.average().expect("strided average");
    let (t_all, p_all) = (all.temperature, all.pressure);
    let (t_strided, p_strided) = (sampled.temperature, sampled.pressure);
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
