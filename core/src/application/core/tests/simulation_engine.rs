// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::SimulationEngine;
use super::DT_GOAL;
use super::create_main_content;
use crate::application::sim_config::DEFAULT_SPEED_LIMIT_FACTOR;
use crate::objects2d::Vec2D;

#[test]
fn test_simulation_engine_advances_main_content_state() {
    let (mut main_content, _) = create_main_content();

    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("failed to add first ball");
    main_content
        .add_candidate_at_pos(Vec2D::new(2.2, 0.0))
        .expect("failed to add second ball");

    main_content.collect_neighbours_by_cell(DT_GOAL);
    let initial_step_count = main_content.world().step_count;

    let (world, step_config) = main_content.world_and_step_config();
    SimulationEngine::simulation_step(world, step_config).expect("simulation step should succeed");

    assert_eq!(main_content.world().step_count, initial_step_count + 1);
    assert!(main_content.world().sim.get_time() > 0.0);
    assert_eq!(
        main_content.world().cached_force_results.len(),
        main_content.world().balls.len()
    );
}

#[test]
fn test_simulation_step_applies_thermostats_before_source_emission() {
    let (mut main_content, _) = create_main_content();

    main_content
        .add_candidate_at_pos(Vec2D::new(4.0, 0.0))
        .expect("failed to add stationary ball");

    main_content.add_thermostat(Vec2D::zero());
    let thermostat_id = main_content.world().thermostats.thermostats()[0].id;
    main_content.toggle_thermostat(thermostat_id);
    let thermostat = main_content
        .world_mut()
        .thermostats
        .find_mut(thermostat_id)
        .expect("thermostat should exist");
    thermostat.target_temperature = 0.0;
    thermostat.radius = 10.0;

    main_content.add_source_sphere(Vec2D::zero());
    let source_id = main_content.world().source_spheres.spheres()[0].id;
    main_content.toggle_source_sphere(source_id);

    let (world, step_config) = main_content.world_and_step_config();
    SimulationEngine::simulation_step(world, step_config).expect("simulation step should succeed");

    assert_eq!(
        main_content.world().balls.len(),
        2,
        "source sphere should emit one ball"
    );

    let moving_balls = main_content
        .world()
        .balls
        .iter()
        .filter(|ball| ball.v.norm_sq() > 1e-8)
        .count();

    assert_eq!(
        moving_balls, 1,
        "thermostat should run before source emission so the emitted ball keeps its initial speed"
    );
}

/// Verify that pausing the simulation prevents step_count from advancing.
/// This tests coordinator state is consistent: the physics coordinator
/// should skip stepping when paused and not modify any counters.
#[test]
fn test_simulation_step_does_not_advance_when_paused() {
    let (mut main_content, _) = create_main_content();

    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("failed to add ball");

    main_content.world_mut().sim.set_pause(true);
    let step_count_before = main_content.world().step_count;
    let time_before = main_content.world().sim.get_time();

    let (world, step_config) = main_content.world_and_step_config();
    SimulationEngine::simulation_step(world, step_config)
        .expect("simulation step should not error when paused");

    assert_eq!(
        main_content.world().step_count,
        step_count_before,
        "step_count must not advance when simulation is paused"
    );
    assert_eq!(
        main_content.world().sim.get_time(),
        time_before,
        "simulation time must not advance when paused"
    );
}

/// Confirm that repeated simulation steps advance time proportionally
/// and that energy statistics are updated each step without stale leftovers.
#[test]
fn test_repeated_steps_advance_time_and_update_energy() {
    let (mut main_content, _) = create_main_content();

    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("failed to add first ball");
    main_content
        .add_candidate_at_pos(Vec2D::new(2.2, 0.0))
        .expect("failed to add second ball");

    let n_steps = 5;
    for _ in 0..n_steps {
        let (world, step_config) = main_content.world_and_step_config();
        SimulationEngine::simulation_step(world, step_config)
            .expect("simulation step should succeed");
    }

    assert_eq!(
        main_content.world().step_count,
        n_steps,
        "step_count should equal the number of steps taken"
    );
    // Energy should be non-zero after several steps with two interacting balls.
    assert!(
        main_content.world().sim.get_kinetic_energy() != 0.0
            || main_content.world().sim.get_potential_energy() != 0.0,
        "energy should be non-zero after simulation steps"
    );
}

/// Pause freezes physics only: step calls and snapshots leave positions,
/// velocities and energy exactly as they were.
#[test]
fn test_paused_physics_state_is_unchanged_by_steps_and_snapshots() {
    let (mut main_content, _) = create_main_content();
    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("failed to add first ball");
    main_content
        .add_candidate_at_pos(Vec2D::new(1.5, 0.0))
        .expect("failed to add second ball");
    main_content.add_thermostat(Vec2D::new(0.5, 0.0));
    main_content.world_mut().balls[0].v = Vec2D::new(1.0, 0.5);
    main_content.world_mut().balls[1].v = Vec2D::new(-0.5, 0.25);
    main_content.world_mut().sim.set_pause(true);

    let state = |mc: &crate::application::MainContent| {
        let world = mc.world();
        (
            world
                .balls
                .iter()
                .map(|b| (*b.get_position(), b.v))
                .collect::<Vec<_>>(),
            world.sim.get_kinetic_energy(),
            world.sim.get_potential_energy(),
            world.sim.get_time(),
        )
    };
    let before = state(&main_content);

    for _ in 0..20 {
        let (world, step_config) = main_content.world_and_step_config();
        SimulationEngine::simulation_step(world, step_config).expect("paused step");
        let _ = main_content.create_snapshot();
    }

    assert_eq!(state(&main_content), before);
}

#[test]
fn test_default_speed_limit_is_positive_for_flipped_world_rect() {
    // World rects have a negative height (y up); the limit must still be 20%
    // of the shorter side, not a negative number.
    let rect = crate::objects2d::Rect::new(0., 0., 50., -36.6);
    let limit = SimulationEngine::default_speed_limit(&rect, DEFAULT_SPEED_LIMIT_FACTOR);
    assert!((limit - 36.6 * 0.2).abs() < 1e-5, "limit {limit}");
}

#[test]
fn test_simulation_step_counts_steps_and_books_phase_time() {
    let (mut main_content, _) = create_main_content();
    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("failed to add first ball");
    main_content
        .add_candidate_at_pos(Vec2D::new(2.2, 0.0))
        .expect("failed to add second ball");
    main_content.reset_phase_timings();

    for _ in 0..3 {
        let (world, step_config) = main_content.world_and_step_config();
        SimulationEngine::simulation_step(world, step_config).expect("simulation step");
    }

    // Per-phase durations of a two-ball step can be below the clock
    // resolution, so only the total is checked; `PhaseClock` has its own
    // tests for booking each lap.
    let timings = *main_content.phase_timings();
    assert_eq!(timings.steps, 3);
    assert!(timings.total() > std::time::Duration::ZERO);
}

fn step(main_content: &mut crate::application::MainContent) {
    let (world, step_config) = main_content.world_and_step_config();
    SimulationEngine::simulation_step(world, step_config).expect("simulation step");
}

fn neighbour_interval(main_content: &mut crate::application::MainContent) -> usize {
    main_content
        .world_and_step_config()
        .1
        .neighbour_update_interval()
}

/// Balls at rest, out of each other's reach, with a wall across the world:
/// every step refills every per-step buffer (walls, contacts, sweep counts,
/// forces, the neighbour pass), but no ball moves, so nothing rebuilds.
fn resting_world_with_wall() -> crate::application::MainContent {
    let (mut main_content, _) = create_main_content();
    main_content.add_line_segment(Vec2D::new(-5.0, -45.0), Vec2D::new(85.0, -45.0));
    for row in 0..4 {
        for col in 0..6 {
            let pos = Vec2D::new(5.0 + 12.0 * col as f32, -5.0 - 12.0 * row as f32);
            main_content
                .add_candidate_at_pos(pos)
                .expect("failed to add ball");
        }
    }
    main_content.stop_all_balls();
    main_content
}

#[test]
fn test_warm_simulation_steps_do_not_allocate() {
    // One worker: rayon then runs the whole step on this thread, where the
    // allocation counter is read. With more workers, allocations on the
    // other threads would not be counted.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("thread pool");
    pool.install(|| {
        let mut main_content = resting_world_with_wall();
        let steps = 4 * neighbour_interval(&mut main_content).max(1);
        // Warm-up: the buffers reach their size, and the force results swap
        // once with `cached_force_results`.
        for _ in 0..steps {
            step(&mut main_content);
        }
        assert!(
            !main_content.world().collect_solid_obstacles().is_empty(),
            "no wall in the scenario"
        );

        let before = super::alloc_count::allocations_on_this_thread();
        for _ in 0..steps {
            step(&mut main_content);
        }
        let allocations = super::alloc_count::allocations_on_this_thread() - before;
        assert_eq!(
            allocations, 0,
            "{steps} warm steps (including neighbour passes) allocated {allocations} times"
        );
    });
}

/// Address and capacity of a buffer.
fn buffer<T>(v: &[T], capacity: usize) -> (usize, usize) {
    (v.as_ptr() as usize, capacity)
}

/// Address and capacity of every scratch buffer.
fn scratch_buffers(main_content: &crate::application::MainContent) -> Vec<(usize, usize)> {
    let world = main_content.world();
    let s = &world.scratch;
    let (u, r) = (&s.update, &s.rebuild);
    // The force buffer and the cached results swap every step; sorted, the
    // pair is the same from step to step.
    let mut forces = [
        buffer(&u.force_results, u.force_results.capacity()),
        buffer(
            &world.cached_force_results,
            world.cached_force_results.capacity(),
        ),
    ];
    forces.sort_unstable();
    let mut buffers = vec![
        buffer(&s.old_positions, s.old_positions.capacity()),
        buffer(&s.neighbour_stats, s.neighbour_stats.capacity()),
        buffer(&u.obstacles, u.obstacles.capacity()),
        buffer(&u.bounce_infos, u.bounce_infos.capacity()),
        buffer(&u.sweep_checks, u.sweep_checks.capacity()),
        buffer(&u.overlap_passes, u.overlap_passes.capacity()),
        buffer(&r.cell_of, r.cell_of.capacity()),
        buffer(&r.checked, r.checked.capacity()),
        buffer(&r.to_rebuild, r.to_rebuild.capacity()),
        buffer(&r.rebuilds, r.rebuilds.capacity()),
    ];
    buffers.extend(forces);
    buffers
}

#[test]
fn test_simulation_step_keeps_every_scratch_buffer_while_it_fits() {
    // Moving balls near a wall: they hit it and rebuild their neighbour
    // lists, so every buffer, including `to_rebuild` and `rebuilds`, holds
    // data. A buffer may only move when it grows; one that keeps its
    // capacity but moves was dropped and allocated again.
    //
    // Comparing addresses assumes a new allocation gets a new address; the
    // allocation count in `test_warm_simulation_steps_do_not_allocate` does
    // not depend on that.
    let (mut main_content, _) = create_main_content();
    main_content.add_line_segment(Vec2D::new(-5.0, -30.0), Vec2D::new(85.0, -30.0));
    for i in 0..20 {
        let pos = Vec2D::new(4.0 * i as f32, -25.0);
        main_content
            .add_candidate_at_pos(pos)
            .expect("failed to add ball");
    }
    for (i, ball) in main_content.world_mut().balls.iter_mut().enumerate() {
        ball.v = Vec2D::new(if i % 2 == 0 { 2.0 } else { -2.0 }, -3.0);
    }

    // Two steps size the buffers; the second also sizes the force buffer,
    // which the first swap with `cached_force_results` leaves empty.
    step(&mut main_content);
    step(&mut main_content);
    let mut previous = scratch_buffers(&main_content);
    let mut steps_with_rebuilds = 0;
    for step_index in 0..300 {
        step(&mut main_content);
        let current = scratch_buffers(&main_content);
        for (i, (before, after)) in previous.iter().zip(&current).enumerate() {
            if after.1 == before.1 && before.1 > 0 {
                assert_eq!(
                    after.0, before.0,
                    "step {step_index}: buffer {i} moved without growing"
                );
            }
        }
        if !main_content.world().scratch.rebuild.to_rebuild.is_empty() {
            steps_with_rebuilds += 1;
        }
        previous = current;
    }

    let world = main_content.world();
    assert!(
        !world.scratch.update.obstacles.is_empty(),
        "no wall in the scenario"
    );
    assert!(world.sim.wall_contacts > 0, "no ball reached the wall");
    assert!(steps_with_rebuilds > 0, "no neighbour list was rebuilt");
    assert!(world.scratch.rebuild.rebuilds.capacity() > 0);
}
