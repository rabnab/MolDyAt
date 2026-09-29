//! Scenario C: deterministic dense-gas run (many-body interactions).
//!
//! Stresses Verlet neighbour lists and cumulative many-body drift. Balls start
//! on a grid in the centre of the world so the horizon ends before any ball
//! reaches a wall; wall effects are covered by `wall_reflection`.

use super::*;

const SIDE: usize = 16;
const SPACING: f32 = 2.4;
const THERMAL_V: f32 = 1.0;
const STEPS: usize = 1_000;

// Thresholds: see docs/drift-test.md for the calibration process.
const MAX_REL_ENERGY_DRIFT: f32 = 1.5e-3;
const MAX_REL_MOMENTUM_DRIFT: f32 = 1.0e-5;
const MIN_BALL_REBUILD_RATE: f32 = 5.0e-3;
const MAX_BALL_REBUILD_RATE: f32 = 5.0e-2;
const DEEP_OVERLAP_DEPTH: f32 = 0.3;
const MAX_DEEP_OVERLAP_STREAK: usize = 2;
const MIN_WALL_CLEARANCE: f32 = 1.0;

/// Places `SIDE * SIDE` balls on a square grid around the world centre.
/// Velocities are deterministic and the total momentum is removed.
fn populate_central_grid(main_content: &mut MainContent) {
    let factory = BallFactory::new();
    let world_rect = main_content.world().world_region();
    let (tl_x, tl_y) = world_rect.get_tl();
    let (br_x, br_y) = world_rect.get_br();
    let center = Vec2D::new((tl_x + br_x) * 0.5, (tl_y + br_y) * 0.5);
    let half = (SIDE as f32 - 1.0) * SPACING * 0.5;

    let mut velocities = Vec::with_capacity(SIDE * SIDE);
    for i in 0..SIDE {
        for j in 0..SIDE {
            let vx = (i as f32 * 0.617 + j as f32 * 1.271).sin() * THERMAL_V;
            let vy = (i as f32 * 1.314 + j as f32 * 0.459).cos() * THERMAL_V;
            velocities.push(Vec2D::new(vx, vy));
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
            dx.min(dy) - b.radius
        })
        .fold(f32::INFINITY, f32::min)
}

/// Checks every dense-gas assertion and returns all violations.
fn dense_gas_violations(metrics: &DriftRunMetrics, min_clearance: f32) -> Vec<String> {
    let mut violations = Vec::new();

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

    let missed = metrics.max_missed_neighbor_pairs();
    if missed > 0 {
        let first = metrics
            .samples
            .iter()
            .find(|s| s.missed_neighbor_pairs > 0)
            .expect("sample with missed pairs");
        violations.push(format!(
            "stale neighbour lists: {missed} in-cutoff pairs missing (first at step {})",
            first.step
        ));
    }

    let streak = metrics.longest_overlap_streak(DEEP_OVERLAP_DEPTH);
    if streak > MAX_DEEP_OVERLAP_STREAK {
        let max_depth = metrics
            .samples
            .iter()
            .map(|s| s.max_overlap_depth)
            .fold(0.0f32, f32::max);
        violations.push(format!(
            "persistent deep overlap: {streak} consecutive samples deeper than {DEEP_OVERLAP_DEPTH} (max_depth={max_depth})"
        ));
    }

    if min_clearance < MIN_WALL_CLEARANCE {
        violations.push(format!(
            "scenario invalid: balls reached the wall region (min_clearance={min_clearance}), horizon too long for a wall-free dense-gas run"
        ));
    }

    violations
}

fn run_dense_gas() -> (DriftRunMetrics, f32) {
    let mut main_content = create_dense_harness_with_seed(404);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    populate_central_grid(&mut main_content);

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
    (metrics, min_clearance)
}

#[test]
#[ignore = "suite:drift"]
fn test_dense_gas_many_body_drift() {
    let (metrics, min_clearance) = run_dense_gas();
    let violations = dense_gas_violations(&metrics, min_clearance);
    metrics.report("dense_gas_many_body", violations.is_empty());
    println!(
        "dense_gas diagnostics: ball_rebuild_events={} min_wall_clearance={min_clearance:.3}",
        metrics.ball_rebuild_events
    );
    assert!(
        violations.is_empty(),
        "dense-gas scenario failed:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
#[ignore = "suite:drift"]
fn test_dense_gas_is_deterministic() {
    let (a, _) = run_dense_gas();
    let (b, _) = run_dense_gas();
    assert_eq!(a.ball_rebuild_events, b.ball_rebuild_events);
    assert_eq!(
        a.final_sample.total_energy.to_bits(),
        b.final_sample.total_energy.to_bits(),
        "dense-gas rerun diverged: {} vs {}",
        a.final_sample.total_energy,
        b.final_sample.total_energy
    );
}

/// Injected regression: a run whose neighbour lists never rebuild and silently
/// drop an in-cutoff pair must be rejected by the same checks.
#[test]
fn test_dense_gas_checks_catch_injected_regressions() {
    let sample = |step, total_energy, missed_neighbor_pairs| DriftSample {
        step,
        kinetic: total_energy,
        potential: 0.0,
        total_energy,
        momentum: Vec2D::new(0.0, 0.0),
        momentum_scale: 1.0,
        max_overlap_depth: 0.0,
        cap_hit_ratio: 0.0,
        ball_count: 10,
        missed_neighbor_pairs,
    };
    let initial = sample(0, 1.0, 0);
    let broken = DriftRunMetrics {
        initial,
        final_sample: sample(100, 1.05, 3),
        samples: vec![initial, sample(50, 1.02, 3), sample(100, 1.05, 3)],
        steps: 100,
        dt: DT_GOAL,
        neighbor_rebuild_count: 0,
        ball_rebuild_events: 0,
    };

    let violations = dense_gas_violations(&broken, f32::INFINITY);
    let joined = violations.join("\n");
    assert!(joined.contains("energy drift envelope"), "{joined}");
    assert!(joined.contains("rebuild cadence"), "{joined}");
    assert!(joined.contains("stale neighbour lists"), "{joined}");
}
