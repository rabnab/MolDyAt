// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Scenario: binary Lennard-Jones mixture (#235).
//!
//! `N2` (σ = ε = 1) and a second species with σ = 1.2 and ε = 1.5, set by
//! pair overrides on the built-in `XS` (mass 1, like `N2`): `XS`–`XS` gets
//! σ = 1.2, ε = 1.5 and `N2`–`XS` the Lorentz–Berthelot values of the two,
//! σ = 1.1, ε = √1.5. Every pair in the run uses an overridden or an `N2`
//! potential, so the per-pair cutoffs, cap distances and shifts of the
//! overrides all take part. The balls start as a checkerboard in the centre
//! of the world and stay clear of the walls.

use super::dense_gas::min_wall_clearance;
use super::neighbor_lists::with_threads;
use super::*;
use crate::application::sim_config::{PairOverrideConfig, SimConfig};
use crate::objects2d::ball_impl::{Ball2D, SpeciesTable};

const SEED: u64 = 606;
const SIDE: usize = 16;
/// Nearest neighbours on the checkerboard are unlike pairs, whose minimum is
/// at 2^(1/6) · 1.1 ≈ 1.23.
const SPACING: f32 = 1.3;
const THERMAL_V: f32 = 1.5;
const STEPS: usize = 1_000;

/// `XS`–`XS`: σ ratio 1 : 1.2 and ε ratio 1 : 1.5 to `N2`.
const SECOND_SIGMA: f32 = 1.2;
const SECOND_EPSILON: f32 = 1.5;

// Thresholds: see docs/drift-test.md for the calibration process.
/// ~1.8x the 5.67e-4 baseline. Nearby starts (thermal speed 1.4..1.6,
/// spacing 1.28..1.32) give 5.7e-4..2.2e-3, growing with the number of
/// force-cap hits (0..13), so a change of the start state needs a new
/// baseline, not just a rerun.
const MAX_REL_ENERGY_DRIFT: f32 = 1.0e-3;
/// f32 rounding noise (1.8e-8 here, 1.6e-8..5.4e-8 for the nearby starts):
/// ~2x the largest of the nearby starts, the basis in docs/drift-test.md.
const MAX_REL_MOMENTUM_DRIFT: f32 = 1.0e-7;
const MIN_WALL_CLEARANCE: f32 = 1.0;

/// The run config: defaults plus the two overrides.
fn mixture_config() -> SimConfig {
    let mut config = SimConfig::default();
    config.potential.pair_overrides = vec![
        PairOverrideConfig {
            a: "XS".into(),
            b: "XS".into(),
            sigma: Some(SECOND_SIGMA),
            epsilon: Some(SECOND_EPSILON),
        },
        PairOverrideConfig {
            a: "N2".into(),
            b: "XS".into(),
            sigma: Some(0.5 * (1.0 + SECOND_SIGMA)),
            epsilon: Some(SECOND_EPSILON.sqrt()),
        },
    ];
    config
}

/// `SIDE * SIDE` balls on a checkerboard of `N2` and `XS` around the world
/// centre, with deterministic velocities and the total momentum removed.
fn mixture_world() -> MainContent {
    let (template, _) = super::super::create_main_content_for_region(-50.0, 50.0);
    let world_rect = template.world().world_region();
    let mut main_content =
        MainContent::with_config(world_rect, &mixture_config(), Some(SEED)).expect("mixture world");
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);

    let builtin = SpeciesTable::builtin();
    let kinds = ["N2", "XS"].map(|name| builtin.by_name(name).unwrap());
    let center = world_center(&main_content);
    let half = (SIDE as f32 - 1.0) * SPACING * 0.5;
    let mut velocities = Vec::with_capacity(SIDE * SIDE);
    for i in 0..SIDE {
        for j in 0..SIDE {
            let vx = (i as f32 * 0.617 + j as f32 * 1.271).sin() * THERMAL_V;
            let vy = (i as f32 * 1.314 + j as f32 * 0.459).cos() * THERMAL_V;
            velocities.push(Vec2D::new(vx, vy));
        }
    }
    // Equal masses: removing the mean velocity removes the momentum.
    let n = velocities.len() as f32;
    let mean_vx = velocities.iter().map(|v| v.x).sum::<f32>() / n;
    let mean_vy = velocities.iter().map(|v| v.y).sum::<f32>() / n;
    for i in 0..SIDE {
        for j in 0..SIDE {
            let pos = Vec2D::new(
                center.x - half + i as f32 * SPACING,
                center.y - half + j as f32 * SPACING,
            );
            let mut ball = Ball2D::new(i * SIDE + j, pos, kinds[(i + j) % 2]);
            let v = velocities[i * SIDE + j];
            ball.v = Vec2D::new(v.x - mean_vx, v.y - mean_vy);
            main_content.world_mut().balls.push(ball);
        }
    }
    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);
    main_content
}

struct MixtureRun {
    metrics: DriftRunMetrics,
    min_clearance: f32,
}

fn run_mixture() -> MixtureRun {
    let mut main_content = mixture_world();
    let mut min_clearance = min_wall_clearance(&main_content);
    let metrics = run_drift_harness_with_observer(
        &mut main_content,
        DriftHarnessConfig {
            steps: STEPS,
            sample_every: 25,
            dt: DT_GOAL,
        },
        |_, mc| min_clearance = min_clearance.min(min_wall_clearance(mc)),
    );
    MixtureRun {
        metrics,
        min_clearance,
    }
}

fn mixture_violations(run: &MixtureRun) -> Vec<String> {
    let m = &run.metrics;
    let mut violations = Vec::new();
    if m.final_sample.ball_count != m.initial.ball_count {
        violations.push(format!(
            "balls were removed (NaN/invalid?): initial={} final={}",
            m.initial.ball_count, m.final_sample.ball_count
        ));
    }
    let max_drift = m.max_relative_energy_drift();
    if max_drift > MAX_REL_ENERGY_DRIFT {
        violations.push(format!(
            "energy drift envelope exceeded: max_drift={max_drift:.3e} threshold={MAX_REL_ENERGY_DRIFT:.3e} initial_total={}",
            m.initial.total_energy
        ));
    }
    let p_drift = m.relative_momentum_drift();
    if p_drift > MAX_REL_MOMENTUM_DRIFT {
        violations.push(format!(
            "momentum drift too high: drift={p_drift:.3e} threshold={MAX_REL_MOMENTUM_DRIFT:.3e}"
        ));
    }
    if let Some(step) = m.first_missed_neighbor_step() {
        violations.push(format!(
            "stale neighbour lists: up to {} in-cutoff pairs missing from a list (first at step {step})",
            m.max_missed_neighbor_pairs()
        ));
    }
    if let Some(step) = m.first_asymmetric_neighbor_step() {
        violations.push(format!(
            "asymmetric neighbour lists: up to {} pairs listed by only one ball (first at step {step})",
            m.max_asymmetric_neighbor_pairs()
        ));
    }
    if run.min_clearance < MIN_WALL_CLEARANCE {
        violations.push(format!(
            "scenario invalid: balls reached the wall region (min_clearance={})",
            run.min_clearance
        ));
    }
    violations
}

/// The overrides reach the physics: the pairs of the run have the configured
/// parameters, not the mixed ones of the built-in `XS`.
#[test]
fn test_binary_mixture_uses_the_overridden_pairs() {
    let main_content = mixture_world();
    let builtin = SpeciesTable::builtin();
    let [n2, xs] = ["N2", "XS"].map(|name| builtin.by_name(name).unwrap());
    let physics = &main_content.world().physics;
    let cutoff_factor = physics.cap_settings().cutoff_factor;
    assert_eq!(physics.cutoff(xs, xs), cutoff_factor * SECOND_SIGMA);
    assert_eq!(physics.cutoff(n2, xs), cutoff_factor * 1.1);
    assert_eq!(physics.cutoff(n2, n2), cutoff_factor * 1.0);
    // Built-in XS alone has σ ≈ 1.27 and ε = 1.
    assert_ne!(builtin.species(xs).sigma, SECOND_SIGMA);
    assert_eq!(main_content.config(), mixture_config());
}

#[test]
#[ignore = "suite:drift"]
fn test_binary_mixture_drift() {
    let run = run_mixture();
    let violations = mixture_violations(&run);
    let name = "binary_mixture";
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
    println!(
        "binary_mixture diagnostics: initial_total={} initial_kinetic={} min_wall_clearance={:.3}",
        m.initial.total_energy, m.initial.kinetic, run.min_clearance
    );
    assert!(
        violations.is_empty(),
        "binary-mixture scenario failed:\n  {}",
        violations.join("\n  ")
    );
}

/// Same trajectory, bit for bit, on 1 and 4 Rayon threads.
#[test]
#[ignore = "suite:drift"]
fn test_binary_mixture_does_not_depend_on_thread_count() {
    let hash = |threads| with_threads(threads, || run_mixture().metrics.trajectory_hash);
    assert_eq!(hash(1), hash(4), "binary mixture: 1 vs 4 threads diverged");
}
