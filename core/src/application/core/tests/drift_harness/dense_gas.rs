// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Scenario C: deterministic dense-gas run (many-body interactions).
//!
//! Stresses Verlet neighbour lists and cumulative many-body drift. Balls start
//! on a grid in the centre of the world so the horizon ends before any ball
//! reaches a wall; wall effects are covered by `wall_reflection`. The gas is
//! hot enough that some pairs reach the force-cap region, so the overlap and
//! cap checks are actually exercised.

use super::*;
use crate::application::core::TestFaults;

const SIDE: usize = 16;
const SPACING: f32 = 2.4;
const THERMAL_V: f32 = 1.0;
const STEPS: usize = 1_000;
/// Grid columns/rows of head-on projectile pairs: ball (i, j) moves +x and
/// (i + 1, j) moves -x, so a few pairs are driven into the force-cap region
/// while the bulk gas stays cool enough not to reach the walls.
const PROJECTILE_SLOTS: [usize; 3] = [4, 8, 11];
/// Head-on at ±3.5 a pair brings ≈ 21·ε, which drives it well past
/// `U(r_cap) ≈ 14.5·ε` of the `M`–`M` pair into the sloped core (#234; the
/// edge-distance core of the size classes started at ≈ 4.4·ε, which ±2.0
/// reached, and ±2.0 no longer gets there).
const PROJECTILE_V: f32 = 3.5;

// Thresholds: see docs/drift-test.md for the calibration process.
const MAX_REL_ENERGY_DRIFT: f32 = 4.0e-3;
/// ~3x baseline: this is f32 rounding noise, 1.7e-8..7.4e-8 across nearby
/// initial velocities (see docs/drift-test.md), plus cross-target headroom.
const MAX_REL_MOMENTUM_DRIFT: f32 = 1.5e-7;
const MIN_BALL_REBUILD_RATE: f32 = 5.0e-3;
const MAX_BALL_REBUILD_RATE: f32 = 5.0e-2;
/// Overlap deeper than this fraction of the ball radius counts as "deep".
const DEEP_OVERLAP_RADIUS_FRACTION: f32 = 0.5;
/// Steps the same pair may stay deeply overlapped before it is "persistent".
const MAX_DEEP_OVERLAP_STREAK: usize = 5;
/// Scenario validity: the run must reach the force-cap region at least this often.
const MIN_CAP_HITS: usize = 1;
const MIN_WALL_CLEARANCE: f32 = 1.0;
/// Max energy drift at dt/4 over that at dt: 1/16 for dt^2 convergence,
/// baseline 0.064, ~2x margin.
const DENSE_GAS_MAX_DT_QUARTER_RATIO: f32 = 0.125;

/// Places `SIDE * SIDE` balls on a square grid around the world centre.
/// Velocities are deterministic and the total momentum is removed.
fn populate_central_grid(main_content: &mut MainContent, thermal_v: f32, projectiles: bool) {
    let factory = scenario_factory();
    let world_rect = main_content.world().world_region();
    let (tl_x, tl_y) = world_rect.get_tl();
    let (br_x, br_y) = world_rect.get_br();
    let center = Vec2D::new((tl_x + br_x) * 0.5, (tl_y + br_y) * 0.5);
    let half = (SIDE as f32 - 1.0) * SPACING * 0.5;

    let mut velocities = Vec::with_capacity(SIDE * SIDE);
    for i in 0..SIDE {
        for j in 0..SIDE {
            let vx = (i as f32 * 0.617 + j as f32 * 1.271).sin() * thermal_v;
            let vy = (i as f32 * 1.314 + j as f32 * 0.459).cos() * thermal_v;
            velocities.push(Vec2D::new(vx, vy));
        }
    }
    for &i in PROJECTILE_SLOTS.iter().filter(|_| projectiles) {
        for &j in &PROJECTILE_SLOTS {
            velocities[i * SIDE + j] = Vec2D::new(PROJECTILE_V, 0.0);
            velocities[(i + 1) * SIDE + j] = Vec2D::new(-PROJECTILE_V, 0.0);
        }
    }
    let n = velocities.len() as f32;
    let mean_vx = velocities.iter().map(|v| v.x).sum::<f32>() / n;
    let mean_vy = velocities.iter().map(|v| v.y).sum::<f32>() / n;

    for i in 0..SIDE {
        for j in 0..SIDE {
            let pos = Vec2D::new(
                center.x - half + i as f32 * SPACING,
                center.y - half + j as f32 * SPACING,
            );
            let v = velocities[i * SIDE + j];
            let vel = Vec2D::new(v.x - mean_vx, v.y - mean_vy);
            let ball = factory.create_massive_with_velocity(pos, vel);
            main_content.world_mut().balls.push(ball);
        }
    }

    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);
}

/// Smallest distance of any ball edge to the world boundary.
fn min_wall_clearance(main_content: &MainContent) -> f32 {
    let world_rect = main_content.world().world_region();
    let (tl_x, tl_y) = world_rect.get_tl();
    let (br_x, br_y) = world_rect.get_br();
    main_content
        .world()
        .balls
        .iter()
        .map(|b| {
            let p = b.get_position();
            let dx = (p.x - tl_x).min(br_x - p.x);
            let dy = (tl_y - p.y).min(p.y - br_y);
            dx.min(dy) - b.contact_radius
        })
        .fold(f32::INFINITY, f32::min)
}

/// Checks every dense-gas assertion and returns all violations.
fn dense_gas_violations(
    metrics: &DriftRunMetrics,
    min_clearance: f32,
    ball_radius: f32,
) -> Vec<String> {
    let mut violations = Vec::new();

    if metrics.final_sample.ball_count != metrics.initial.ball_count {
        violations.push(format!(
            "balls were removed (NaN/invalid?): initial={} final={}",
            metrics.initial.ball_count, metrics.final_sample.ball_count
        ));
    }

    let max_drift = metrics.max_relative_energy_drift();
    if max_drift > MAX_REL_ENERGY_DRIFT {
        let worst = metrics
            .samples
            .iter()
            .max_by(|a, b| {
                let da = (a.total_energy - metrics.initial.total_energy).abs();
                let db = (b.total_energy - metrics.initial.total_energy).abs();
                da.total_cmp(&db)
            })
            .expect("samples");
        violations.push(format!(
            "energy drift envelope exceeded: max_drift={max_drift:.3e} threshold={MAX_REL_ENERGY_DRIFT:.3e} worst_step={} initial_total={} worst_total={}",
            worst.step, metrics.initial.total_energy, worst.total_energy
        ));
    }

    let p_drift = metrics.relative_momentum_drift();
    if p_drift > MAX_REL_MOMENTUM_DRIFT {
        violations.push(format!(
            "momentum drift too high: drift={p_drift:.3e} threshold={MAX_REL_MOMENTUM_DRIFT:.3e} initial_p={:?} final_p={:?}",
            metrics.initial.momentum, metrics.final_sample.momentum
        ));
    }

    let rate = metrics.ball_rebuild_rate();
    if !(MIN_BALL_REBUILD_RATE..=MAX_BALL_REBUILD_RATE).contains(&rate) {
        violations.push(format!(
            "neighbour rebuild cadence pathological: ball_rebuild_rate={rate:.3e} expected=[{MIN_BALL_REBUILD_RATE:.1e}, {MAX_BALL_REBUILD_RATE:.1e}] ball_rebuild_events={} rebuild_steps={} steps={}",
            metrics.ball_rebuild_events, metrics.neighbor_rebuild_count, metrics.steps
        ));
    }

    if let Some(step) = metrics.first_missed_neighbor_step() {
        violations.push(format!(
            "stale neighbour lists: up to {} in-cutoff pairs missing from a list (first at step {step})",
            metrics.max_missed_neighbor_pairs()
        ));
    }

    if let Some(step) = metrics.first_asymmetric_neighbor_step() {
        violations.push(format!(
            "asymmetric neighbour lists: up to {} pairs listed by only one ball (first at step {step})",
            metrics.max_asymmetric_neighbor_pairs()
        ));
    }

    let deep = DEEP_OVERLAP_RADIUS_FRACTION * ball_radius;
    let (streak, pair) = metrics.longest_overlap_streak(deep);
    if streak > MAX_DEEP_OVERLAP_STREAK {
        violations.push(format!(
            "persistent deep overlap: pair {pair:?} deeper than {deep} for {streak} consecutive steps (max_depth={})",
            metrics.max_overlap_depth()
        ));
    }

    if metrics.total_cap_hits() < MIN_CAP_HITS {
        violations.push(format!(
            "scenario invalid: no pair reached the force-cap region (cap_hits={} min_edge_distance={}), overlap/cap checks would be vacuous",
            metrics.total_cap_hits(),
            metrics.min_edge_distance()
        ));
    }

    if min_clearance < MIN_WALL_CLEARANCE {
        violations.push(format!(
            "scenario invalid: balls reached the wall region (min_clearance={min_clearance}), horizon too long for a wall-free dense-gas run"
        ));
    }

    violations
}

pub(super) struct DenseGasRun {
    pub(super) metrics: DriftRunMetrics,
    min_clearance: f32,
    ball_radius: f32,
}

impl DenseGasRun {
    fn violations(&self) -> Vec<String> {
        dense_gas_violations(&self.metrics, self.min_clearance, self.ball_radius)
    }
}

pub(super) fn run_dense_gas(faults: TestFaults) -> DenseGasRun {
    run_dense_gas_setup(faults, true)
}

fn run_dense_gas_setup(faults: TestFaults, projectiles: bool) -> DenseGasRun {
    run_dense_gas_timed(faults, projectiles, DT_GOAL, STEPS, 25)
}

/// The dense gas over `steps` steps of `dt`, sampled every `sample_every` steps.
fn run_dense_gas_timed(
    faults: TestFaults,
    projectiles: bool,
    dt: f32,
    steps: usize,
    sample_every: usize,
) -> DenseGasRun {
    let mut main_content = create_dense_harness_with_seed(404);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    main_content.world_mut().test_faults = faults;
    populate_central_grid(&mut main_content, THERMAL_V, projectiles);
    let ball_radius = main_content.world().balls[0].contact_radius;

    let mut min_clearance = min_wall_clearance(&main_content);
    let metrics = run_drift_harness_with_observer(
        &mut main_content,
        DriftHarnessConfig {
            steps,
            sample_every,
            dt,
        },
        |_, mc| min_clearance = min_clearance.min(min_wall_clearance(mc)),
    );
    DenseGasRun {
        metrics,
        min_clearance,
        ball_radius,
    }
}

#[test]
#[ignore = "suite:drift"]
fn test_dense_gas_many_body_drift() {
    let run = run_dense_gas(TestFaults::default());
    let violations = run.violations();
    let name = "dense_gas_many_body";
    run.metrics.report(name, violations.is_empty());
    let m = &run.metrics;
    warn_near_threshold(
        name,
        "max_rel_energy_drift",
        m.max_relative_energy_drift(),
        MAX_REL_ENERGY_DRIFT,
    );
    warn_near_threshold(
        name,
        "rel_momentum_drift",
        m.relative_momentum_drift(),
        MAX_REL_MOMENTUM_DRIFT,
    );
    warn_near_threshold(
        name,
        "ball_rebuild_rate",
        m.ball_rebuild_rate(),
        MAX_BALL_REBUILD_RATE,
    );
    println!(
        "dense_gas diagnostics: ball_rebuild_events={} min_wall_clearance={:.3}",
        run.metrics.ball_rebuild_events, run.min_clearance
    );
    assert!(
        violations.is_empty(),
        "dense-gas scenario failed:\n  {}",
        violations.join("\n  ")
    );
}

/// Regression for asynchronous per-ball Verlet rebuilds: with the old skin/2
/// movement threshold this cool gas (no projectiles) had an in-cutoff pair
/// missing from both lists at step 186. skin/3 must keep every pair listed.
#[test]
#[ignore = "suite:drift"]
fn test_cool_dense_gas_has_no_stale_neighbor_lists() {
    let run = run_dense_gas_setup(TestFaults::default(), false);
    assert_eq!(
        run.metrics.max_missed_neighbor_pairs(),
        0,
        "stale neighbour lists in cool dense gas (first at step {:?})",
        run.metrics.first_missed_neighbor_step()
    );
    assert_eq!(run.metrics.max_asymmetric_neighbor_pairs(), 0);

    // The same setup with the old skin/2 threshold (1.5x skin/3) goes stale.
    let old = run_dense_gas_setup(
        TestFaults {
            neighbor_movement_scale: 1.5,
            ..TestFaults::default()
        },
        false,
    );
    assert!(
        old.metrics.max_missed_neighbor_pairs() > 0,
        "old skin/2 threshold no longer reproduces the stale pair; regression test is vacuous"
    );
}

/// Quartering dt over the same physical time must shrink the energy envelope
/// about as dt^2 (velocity Verlet), so many-body drift is integrator error and
/// not a list or cap artifact (the two-body convergence check cannot see
/// those). This needs the drift world at the origin: far from it, f32 position
/// rounding dominates and the envelope shrinks only about linearly (see
/// docs/drift-test.md). dt/4 rather than dt/2 for a larger, clearer ratio.
#[test]
#[ignore = "suite:drift"]
fn test_dense_gas_drift_converges_with_smaller_timestep() {
    let coarse = run_dense_gas_timed(TestFaults::default(), true, DT_GOAL, STEPS, 25);
    let fine = run_dense_gas_timed(TestFaults::default(), true, DT_GOAL * 0.25, 4 * STEPS, 100);
    let coarse_drift = coarse.metrics.max_relative_energy_drift();
    let fine_drift = fine.metrics.max_relative_energy_drift();
    let ratio = fine_drift / coarse_drift.max(1e-12);
    println!(
        "DRIFT_CONVERGENCE name=dense_gas_dt_quartering coarse_dt={DT_GOAL} coarse_max_rel_energy_drift={coarse_drift:.3e} fine_max_rel_energy_drift={fine_drift:.3e} ratio={ratio:.3} threshold={DENSE_GAS_MAX_DT_QUARTER_RATIO}"
    );
    warn_near_threshold(
        "dense_gas_dt_quartering",
        "drift_ratio",
        ratio,
        DENSE_GAS_MAX_DT_QUARTER_RATIO,
    );
    let fine_list_problems: Vec<_> = fine
        .violations()
        .into_iter()
        .filter(|v| v.contains("neighbour lists"))
        .collect();
    assert!(
        fine_list_problems.is_empty(),
        "fine run: {fine_list_problems:?}"
    );
    assert!(
        ratio <= DENSE_GAS_MAX_DT_QUARTER_RATIO,
        "dense gas does not converge with dt: coarse={coarse_drift:.3e} fine={fine_drift:.3e} ratio={ratio:.3} > {DENSE_GAS_MAX_DT_QUARTER_RATIO}"
    );
}

/// In-process reproducibility only: two runs in the same binary must produce a
/// bit-identical trajectory. This says nothing about other runners or SIMD
/// targets (SSE2/AVX2/NEON), which is why thresholds, not bit equality, gate CI.
#[test]
#[ignore = "suite:drift"]
fn test_dense_gas_is_deterministic() {
    let a = run_dense_gas(TestFaults::default());
    let b = run_dense_gas(TestFaults::default());
    assert_eq!(a.metrics.ball_rebuild_events, b.metrics.ball_rebuild_events);
    assert_eq!(
        a.metrics.trajectory_hash, b.metrics.trajectory_hash,
        "dense-gas rerun diverged: final_total {} vs {}",
        a.metrics.final_sample.total_energy, b.metrics.final_sample.total_energy
    );
}

/// Real-simulation injected regression: a 10x Verlet movement threshold lets
/// neighbour lists go stale, and the scenario must reject it.
#[test]
#[ignore = "suite:drift"]
fn test_dense_gas_catches_injected_stale_neighbor_lists() {
    let run = run_dense_gas(TestFaults {
        neighbor_movement_scale: 10.0,
        ..TestFaults::default()
    });
    let joined = run.violations().join("\n");
    assert!(joined.contains("stale neighbour lists"), "{joined}");
    assert!(joined.contains("rebuild cadence"), "{joined}");
}

/// Injected regression on synthetic metrics: never-rebuilding, stale,
/// asymmetric lists with a persistent overlap and lost balls must be rejected.
#[test]
fn test_dense_gas_checks_catch_injected_regressions() {
    let sample = |step, total_energy, ball_count| DriftSample {
        step,
        kinetic: total_energy,
        potential: 0.0,
        total_energy,
        momentum: Vec2D::new(0.0, 0.0),
        momentum_scale: 1.0,
        max_overlap_depth: 0.0,
        cap_hit_ratio: 0.0,
        ball_count,
    };
    let pair_history = (0..=100)
        .map(|step| PairStepStats {
            step,
            missed_neighbor_pairs: 3,
            asymmetric_neighbor_pairs: 1,
            deepest_overlap: Some((1, 2, 0.5)),
            cap_hits: 0,
            pair_count: 45,
            min_edge_distance: -0.5,
        })
        .collect();
    let initial = sample(0, 1.0, 10);
    let broken = DriftRunMetrics {
        initial,
        final_sample: sample(100, 1.05, 9),
        samples: vec![initial, sample(50, 1.02, 10), sample(100, 1.05, 9)],
        pair_history,
        steps: 100,
        dt: DT_GOAL,
        neighbor_rebuild_count: 0,
        ball_rebuild_events: 0,
        trajectory_hash: 0,
    };

    let violations = dense_gas_violations(&broken, f32::INFINITY, 0.6);
    let joined = violations.join("\n");
    for expected in [
        "balls were removed",
        "energy drift envelope",
        "rebuild cadence",
        "stale neighbour lists",
        "asymmetric neighbour lists",
        "persistent deep overlap",
        "scenario invalid: no pair reached the force-cap",
    ] {
        assert!(
            joined.contains(expected),
            "missing '{expected}' in:\n{joined}"
        );
    }
}
