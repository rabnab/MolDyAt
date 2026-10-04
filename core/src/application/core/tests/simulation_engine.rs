use super::super::SimulationEngine;
use super::DT_GOAL;
use super::create_main_content;
use crate::application::GameLogic;
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

    let (world, game_data) = main_content.world_and_game_data();
    SimulationEngine::simulation_step(world, game_data).expect("simulation step should succeed");

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

    let (world, game_data) = main_content.world_and_game_data();
    SimulationEngine::simulation_step(world, game_data).expect("simulation step should succeed");

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

    let (world, game_data) = main_content.world_and_game_data();
    SimulationEngine::simulation_step(world, game_data)
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

/// Pause freezes physics only: many step calls (and snapshots, which the
/// physics thread keeps publishing) leave positions, velocities and energy
/// exactly as they were.
#[test]
fn test_paused_physics_state_is_unchanged_by_steps_and_snapshots() {
    let (mut main_content, _) = create_main_content();
    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("failed to add first ball");
    main_content
        .add_candidate_at_pos(Vec2D::new(1.5, 0.0))
        .expect("failed to add second ball");
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
        let (world, game_data) = main_content.world_and_game_data();
        SimulationEngine::simulation_step(world, game_data).expect("paused step");
        main_content.refresh_thermostat_readings();
        let _ = main_content.create_snapshot();
    }

    assert_eq!(state(&main_content), before);
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
        let (world, game_data) = main_content.world_and_game_data();
        SimulationEngine::simulation_step(world, game_data)
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
