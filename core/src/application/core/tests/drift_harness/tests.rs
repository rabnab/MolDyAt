// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use crate::objects2d::ball_impl::ball_physics::PhysicsConstants;
use crate::objects2d::ball_impl::pair_potential::CapSettings;
use crate::objects2d::ball_impl::{BallPhysics, SpeciesTable};

#[test]
#[ignore = "suite:drift"]
fn test_drift_harness_collects_metrics() {
    let mut main_content = create_dense_harness_with_seed(12345);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    populate_grid_balls(&mut main_content, 100, 1.2);

    let cfg = DriftHarnessConfig {
        steps: 20,
        sample_every: 5,
        dt: DT_GOAL,
    };
    let metrics = run_drift_harness(&mut main_content, cfg);

    assert_eq!(metrics.initial.step, 0);
    assert_eq!(metrics.final_sample.step, 20);
    assert!(metrics.samples.len() >= 2);
    assert!(metrics.relative_energy_drift().is_finite());
    assert!(metrics.relative_momentum_drift().is_finite());
}

#[test]
#[ignore = "suite:drift"]
fn test_two_body_lj_baseline_drift() {
    let mut main_content = create_dense_harness_with_seed(101);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);

    let factory = scenario_factory();
    // Just beyond the potential minimum.
    let center_sep = 1.05
        * main_content
            .world()
            .physics
            .equilibrium_distance(SCENARIO_SPECIES, SCENARIO_SPECIES);
    let center = world_center(&main_content);
    let x_off = center_sep * 0.5;

    let b1 = factory
        .create_massive_with_velocity(Vec2D::new(center.x - x_off, center.y), Vec2D::new(0.2, 0.0));
    let b2 = factory.create_massive_with_velocity(
        Vec2D::new(center.x + x_off, center.y),
        Vec2D::new(-0.2, 0.0),
    );

    main_content
        .add_candidate_ball(b1, 0.0)
        .expect("could not add first baseline ball");
    main_content
        .add_candidate_ball(b2, 0.0)
        .expect("could not add second baseline ball");
    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);

    let metrics = run_drift_harness(
        &mut main_content,
        DriftHarnessConfig {
            steps: 2_000,
            sample_every: 100,
            dt: DT_GOAL,
        },
    );

    const MAX_REL_ENERGY_DRIFT: f32 = 3.0e-2;
    const MAX_REL_MOMENTUM_DRIFT: f32 = 3.0e-3;
    metrics.report(
        "two_body_lj_baseline",
        metrics.relative_energy_drift() <= MAX_REL_ENERGY_DRIFT
            && metrics.relative_momentum_drift() <= MAX_REL_MOMENTUM_DRIFT,
    );
    assert!(
        metrics.relative_energy_drift() <= MAX_REL_ENERGY_DRIFT,
        "two-body LJ energy drift too high: drift={}, threshold={}, initial_total={}, final_total={}",
        metrics.relative_energy_drift(),
        MAX_REL_ENERGY_DRIFT,
        metrics.initial.total_energy,
        metrics.final_sample.total_energy
    );
    assert!(
        metrics.relative_momentum_drift() <= MAX_REL_MOMENTUM_DRIFT,
        "two-body LJ relative momentum drift too high: drift={}, threshold={}, initial_p={:?}, final_p={:?}",
        metrics.relative_momentum_drift(),
        MAX_REL_MOMENTUM_DRIFT,
        metrics.initial.momentum,
        metrics.final_sample.momentum
    );
}

#[test]
#[ignore = "suite:drift"]
fn test_force_cap_transition_drift() {
    let mut main_content = create_dense_harness_with_seed(202);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);

    let cap = CapSettings {
        max_force: 10.0,
        ..CapSettings::default()
    };
    main_content.world_mut().physics =
        BallPhysics::new(PhysicsConstants::default(), SpeciesTable::builtin(), cap);

    let factory = scenario_factory();
    let r_cap = main_content
        .world()
        .physics
        .get_force_cap_distance(SCENARIO_SPECIES, SCENARIO_SPECIES);
    let center_sep = r_cap * 0.95;
    let center = world_center(&main_content);
    let x_off = center_sep * 0.5;

    let b1 = factory
        .create_massive_with_velocity(Vec2D::new(center.x - x_off, center.y), Vec2D::new(0.8, 0.0));
    let b2 = factory.create_massive_with_velocity(
        Vec2D::new(center.x + x_off, center.y),
        Vec2D::new(-0.8, 0.0),
    );

    // Inside the core the contact discs overlap, so no placement check.
    main_content.add_candidate_to_vec(b1);
    main_content.add_candidate_to_vec(b2);
    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);

    let metrics = run_drift_harness(
        &mut main_content,
        DriftHarnessConfig {
            steps: 300,
            sample_every: 50,
            dt: DT_GOAL,
        },
    );

    const MAX_REL_ENERGY_DRIFT: f32 = 3.0e-1;
    const MAX_REL_MOMENTUM_DRIFT: f32 = 3.0e-3;
    const MIN_CAP_HIT_RATIO: f32 = 1.0e-2;
    let max_cap_hit_ratio = metrics
        .samples
        .iter()
        .map(|s| s.cap_hit_ratio)
        .fold(0.0f32, f32::max);
    metrics.report(
        "force_cap_transition",
        metrics.relative_energy_drift() <= MAX_REL_ENERGY_DRIFT
            && metrics.relative_momentum_drift() <= MAX_REL_MOMENTUM_DRIFT
            && max_cap_hit_ratio >= MIN_CAP_HIT_RATIO,
    );

    assert!(
        metrics.relative_energy_drift() <= MAX_REL_ENERGY_DRIFT,
        "force-cap transition energy drift too high: drift={}, threshold={}, initial_total={}, final_total={}",
        metrics.relative_energy_drift(),
        MAX_REL_ENERGY_DRIFT,
        metrics.initial.total_energy,
        metrics.final_sample.total_energy
    );
    assert!(
        metrics.relative_momentum_drift() <= MAX_REL_MOMENTUM_DRIFT,
        "force-cap transition relative momentum drift too high: drift={}, threshold={}, initial_p={:?}, final_p={:?}",
        metrics.relative_momentum_drift(),
        MAX_REL_MOMENTUM_DRIFT,
        metrics.initial.momentum,
        metrics.final_sample.momentum
    );
    assert!(
        max_cap_hit_ratio >= MIN_CAP_HIT_RATIO,
        "force-cap transition did not exercise capped region enough: max_cap_hit_ratio={}, threshold={}, r_cap={}",
        max_cap_hit_ratio,
        MIN_CAP_HIT_RATIO,
        r_cap
    );
}

#[test]
#[ignore = "suite:drift"]
fn test_two_body_drift_converges_with_smaller_timestep() {
    let make_two_body = || {
        let mut main_content = create_dense_harness_with_seed(303);
        main_content.world_mut().speed_limit_override = Some(f32::INFINITY);

        let factory = scenario_factory();
        // Just beyond the potential minimum.
        let center_sep = 1.05
            * main_content
                .world()
                .physics
                .equilibrium_distance(SCENARIO_SPECIES, SCENARIO_SPECIES);
        let center = world_center(&main_content);
        let x_off = center_sep * 0.5;

        let b1 = factory.create_massive_with_velocity(
            Vec2D::new(center.x - x_off, center.y),
            Vec2D::new(0.25, 0.0),
        );
        let b2 = factory.create_massive_with_velocity(
            Vec2D::new(center.x + x_off, center.y),
            Vec2D::new(-0.25, 0.0),
        );

        main_content
            .add_candidate_ball(b1, 0.0)
            .expect("could not add first convergence ball");
        main_content
            .add_candidate_ball(b2, 0.0)
            .expect("could not add second convergence ball");
        main_content.world_mut().rebuild_cell_grid();
        main_content.collect_neighbours_by_cell(DT_GOAL);
        main_content
    };

    let total_time = 10.0f32;
    let dt_coarse = DT_GOAL;
    let dt_fine = DT_GOAL * 0.5;
    let coarse_steps = (total_time / dt_coarse).round() as usize;
    let fine_steps = (total_time / dt_fine).round() as usize;

    let mut coarse = make_two_body();
    let coarse_metrics = run_drift_harness(
        &mut coarse,
        DriftHarnessConfig {
            steps: coarse_steps,
            sample_every: (coarse_steps / 10).max(1),
            dt: dt_coarse,
        },
    );

    let mut fine = make_two_body();
    let fine_metrics = run_drift_harness(
        &mut fine,
        DriftHarnessConfig {
            steps: fine_steps,
            sample_every: (fine_steps / 10).max(1),
            dt: dt_fine,
        },
    );

    let coarse_energy_drift = coarse_metrics.relative_energy_drift();
    let fine_energy_drift = fine_metrics.relative_energy_drift();
    let coarse_momentum_drift = coarse_metrics.relative_momentum_drift();
    let fine_momentum_drift = fine_metrics.relative_momentum_drift();
    const CONVERGENCE_MARGIN: f32 = 2.0;
    println!(
        "DRIFT_CONVERGENCE name=two_body_dt_halving coarse_dt={dt_coarse} coarse_rel_energy_drift={coarse_energy_drift:.3e} fine_rel_energy_drift={fine_energy_drift:.3e} coarse_rel_momentum_drift={coarse_momentum_drift:.3e} fine_rel_momentum_drift={fine_momentum_drift:.3e} margin={CONVERGENCE_MARGIN}"
    );

    assert!(
        fine_energy_drift <= coarse_energy_drift * CONVERGENCE_MARGIN,
        "expected finer timestep energy drift <= coarse energy drift * margin; coarse_dt={} coarse_steps={} coarse_energy_drift={} fine_dt={} fine_steps={} fine_energy_drift={} margin={}",
        dt_coarse,
        coarse_steps,
        coarse_energy_drift,
        dt_fine,
        fine_steps,
        fine_energy_drift,
        CONVERGENCE_MARGIN
    );
    assert!(
        fine_momentum_drift <= coarse_momentum_drift * CONVERGENCE_MARGIN,
        "expected finer timestep momentum drift <= coarse momentum drift * margin; coarse_dt={} coarse_steps={} coarse_momentum_drift={} fine_dt={} fine_steps={} fine_momentum_drift={} margin={}",
        dt_coarse,
        coarse_steps,
        coarse_momentum_drift,
        dt_fine,
        fine_steps,
        fine_momentum_drift,
        CONVERGENCE_MARGIN
    );
}
