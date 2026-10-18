// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Scenarios that stress the Verlet neighbour lists rather than the integrator.
//!
//! - Hot, sparse gas: many balls rebuild their lists in the same step, which
//!   exercises the partner updates of the parallel rebuild and the skin/3
//!   movement threshold at high speed.
//! - Mixed sizes at cell borders: pairs of the largest balls placed so that a
//!   3x3 cell search only finds them if no cell is narrower than the
//!   interaction range (`CellGrid::new`).
//! - Thread count: the trajectory must not depend on the Rayon pool size.
//! - Balls removed and added mid-run: lists must be rebuilt correctly after
//!   the ball indices shift (see the index rules in CLAUDE.md).

use super::dense_gas::run_dense_gas;
use super::*;
use crate::application::core::TestFaults;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::{Ball2D, SpeciesId, SpeciesTable};

const HOT_SEED: u64 = 505;
const HOT_BALLS: usize = 400;
const HOT_V: f32 = 6.0;
const HOT_STEPS: usize = 1_000;

// Thresholds: see docs/drift-test.md for the calibration process.
/// ~1.7x the 4.75e-3 baseline.
const HOT_MAX_REL_ENERGY_DRIFT: f32 = 8.0e-3;
/// Band around the 0.22 baseline (about 10x the dense gas).
const HOT_MIN_BALL_REBUILD_RATE: f32 = 1.0e-1;
const HOT_MAX_BALL_REBUILD_RATE: f32 = 4.0e-1;

/// Violations shared by the neighbour-list scenarios: stale or one-sided
/// lists on any recorded step.
fn list_violations(metrics: &DriftRunMetrics) -> Vec<String> {
    let mut violations = Vec::new();
    let checked = || metrics.pair_history.iter();
    if let Some(p) = checked().find(|p| p.missed_neighbor_pairs > 0) {
        violations.push(format!(
            "stale neighbour lists: {} in-cutoff pairs missing from a list at step {}",
            p.missed_neighbor_pairs, p.step
        ));
    }
    if let Some(p) = checked().find(|p| p.asymmetric_neighbor_pairs > 0) {
        violations.push(format!(
            "asymmetric neighbour lists: {} pairs listed by only one ball at step {}",
            p.asymmetric_neighbor_pairs, p.step
        ));
    }
    violations
}

fn hot_gas_violations(metrics: &DriftRunMetrics) -> Vec<String> {
    let mut violations = list_violations(metrics);

    if metrics.final_sample.ball_count != metrics.initial.ball_count {
        violations.push(format!(
            "balls were removed (NaN/invalid?): initial={} final={}",
            metrics.initial.ball_count, metrics.final_sample.ball_count
        ));
    }

    let max_drift = metrics.max_relative_energy_drift();
    if max_drift > HOT_MAX_REL_ENERGY_DRIFT {
        violations.push(format!(
            "energy drift envelope exceeded: max_drift={max_drift:.3e} threshold={HOT_MAX_REL_ENERGY_DRIFT:.3e}"
        ));
    }

    let rate = metrics.ball_rebuild_rate();
    if !(HOT_MIN_BALL_REBUILD_RATE..=HOT_MAX_BALL_REBUILD_RATE).contains(&rate) {
        violations.push(format!(
            "neighbour rebuild cadence pathological: ball_rebuild_rate={rate:.3e} expected=[{HOT_MIN_BALL_REBUILD_RATE:.1e}, {HOT_MAX_BALL_REBUILD_RATE:.1e}]"
        ));
    }
    violations
}

/// A hot gas spread over the whole default drift world. Balls reach the walls
/// (elastic, energy-conserving), so momentum is not checked here.
fn hot_gas_world(faults: TestFaults) -> MainContent {
    let mut main_content = create_dense_harness_with_seed(HOT_SEED);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    main_content.world_mut().test_faults = faults;
    populate_grid_balls(&mut main_content, HOT_BALLS, HOT_V);
    main_content
}

fn hot_gas_config(steps: usize) -> DriftHarnessConfig {
    DriftHarnessConfig {
        steps,
        sample_every: 25,
        dt: DT_GOAL,
    }
}

fn run_hot_gas(faults: TestFaults) -> DriftRunMetrics {
    let mut main_content = hot_gas_world(faults);
    run_drift_harness(&mut main_content, hot_gas_config(HOT_STEPS))
}

#[test]
#[ignore = "suite:drift"]
fn test_hot_sparse_gas_neighbor_lists() {
    let metrics = run_hot_gas(TestFaults::default());
    let violations = hot_gas_violations(&metrics);
    let name = "hot_sparse_gas";
    metrics.report(name, violations.is_empty());
    warn_near_threshold(
        name,
        "max_rel_energy_drift",
        metrics.max_relative_energy_drift(),
        HOT_MAX_REL_ENERGY_DRIFT,
    );
    warn_near_threshold(
        name,
        "ball_rebuild_rate",
        metrics.ball_rebuild_rate(),
        HOT_MAX_BALL_REBUILD_RATE,
    );
    assert!(
        violations.is_empty(),
        "hot sparse gas failed:\n  {}",
        violations.join("\n  ")
    );
}

/// Injected regression: a 10x movement threshold must make the hot gas fail.
#[test]
#[ignore = "suite:drift"]
fn test_hot_sparse_gas_catches_injected_stale_neighbor_lists() {
    let metrics = run_hot_gas(TestFaults {
        neighbor_movement_scale: 10.0,
        ..TestFaults::default()
    });
    let joined = hot_gas_violations(&metrics).join("\n");
    assert!(joined.contains("stale neighbour lists"), "{joined}");
}

/// World of `cells_x` x `cells_y` interaction-range cells (the list radius of
/// the largest pair, `XL`–`XL`): with a fractional count the old rounded-up
/// grid made the cells narrower than that range.
fn cell_border_world(cells_x: f32, cells_y: f32) -> MainContent {
    let physics = crate::objects2d::ball_impl::BallPhysics::default();
    let xl = largest_species();
    let range = physics.list_radius(xl, xl);
    let (w, h) = (cells_x * range, cells_y * range);
    let mut main_content = MainContent::new_with_seed(Rect::new(0.0, h, w, -h), 606)
        .expect("Could not create cell-border world");
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    main_content
}

/// The species with the largest `σ` (`XL`), whose pair sets the cell size.
fn largest_species() -> SpeciesId {
    SpeciesId::new((SpeciesTable::builtin().len() - 1) as u16)
}

/// Pairs of the largest balls just inside the LJ cutoff of each other, the
/// first ball just before a border of the rounded-up grid and the second two
/// such cells further on, plus smaller balls of every other species.
fn populate_cell_border_pairs(main_content: &mut MainContent) -> usize {
    let factory = scenario_factory();
    let world_rect = main_content.world().world_region();
    let (w, h) = (world_rect.w.abs(), world_rect.h.abs());
    let physics = crate::objects2d::ball_impl::BallPhysics::default();
    let xl = largest_species();
    let range = physics.list_radius(xl, xl);
    // Cell sizes of the old grid, which rounded the cell count up.
    let old_w = w / (w / range).ceil();
    let old_h = h / (h / range).ceil();
    let r_max = SpeciesTable::builtin().species(xl).contact_radius();
    let pair_dist = 0.95 * physics.cutoff(xl, xl);
    let max_size = xl.index();
    let slow = |i: usize| Vec2D::new((i as f32 * 1.7).sin() * 0.3, (i as f32 * 0.9).cos() * 0.3);

    let mut balls: Vec<Ball2D> = Vec::new();
    let mut add = |pos: Vec2D, size: usize| {
        let i = balls.len();
        let species = SpeciesId::new(size as u16);
        balls.push(factory.create_with_species_and_velocity(pos, species, slow(i)));
    };
    // Horizontal pair across two old column borders, near the bottom.
    let ax = old_w - 0.1;
    add(Vec2D::new(ax, r_max + 1.0), max_size);
    add(Vec2D::new(ax + pair_dist, r_max + 1.0), max_size);
    // Vertical pair across two old row borders, near the left wall.
    let ay = h - (old_h - 0.1);
    add(Vec2D::new(r_max + 0.5, ay), max_size);
    add(Vec2D::new(r_max + 0.5, ay - pair_dist), max_size);
    // Smaller balls of every other size along the top and right.
    let mut size = 0;
    let mut x = 2.0 * r_max + 3.0;
    while x < w - 1.0 {
        add(Vec2D::new(x, h - 1.0), size % max_size);
        size += 1;
        x += 2.0;
    }
    let mut y = h - 4.0;
    while y > 2.0 * r_max + 4.0 {
        add(Vec2D::new(w - 1.0, y), size % max_size);
        size += 1;
        y -= 2.0;
    }

    let count = balls.len();
    main_content.world_mut().balls.extend(balls);
    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);
    count
}

#[test]
#[ignore = "suite:drift"]
fn test_mixed_sizes_at_cell_borders_keep_every_pair_listed() {
    // 2.2 x 3.3 ranges: the old grid had 3 x 4 cells, each narrower than the
    // largest pair's centre distance; the current grid has 2 x 3.
    let mut main_content = cell_border_world(2.2, 3.3);
    let balls = populate_cell_border_pairs(&mut main_content);
    let metrics = run_drift_harness(
        &mut main_content,
        DriftHarnessConfig {
            steps: 300,
            sample_every: 25,
            dt: DT_GOAL,
        },
    );
    let mut violations = list_violations(&metrics);
    if metrics.final_sample.ball_count != balls {
        violations.push(format!(
            "balls were removed: initial={balls} final={}",
            metrics.final_sample.ball_count
        ));
    }
    metrics.report("mixed_sizes_cell_borders", violations.is_empty());
    assert!(
        violations.is_empty(),
        "mixed sizes at cell borders failed:\n  {}",
        violations.join("\n  ")
    );
}

/// Runs `f` on a dedicated Rayon pool with `threads` workers.
fn with_threads<T: Send>(threads: usize, f: impl FnOnce() -> T + Send) -> T {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("thread pool")
        .install(f)
}

#[test]
#[ignore = "suite:drift"]
fn test_trajectory_does_not_depend_on_thread_count() {
    let hot = |threads| with_threads(threads, || run_hot_gas(TestFaults::default()));
    let (hot_1, hot_4) = (hot(1), hot(4));
    assert_eq!(
        hot_1.trajectory_hash, hot_4.trajectory_hash,
        "hot gas: 1 thread vs 4 threads diverged (final_total {} vs {})",
        hot_1.final_sample.total_energy, hot_4.final_sample.total_energy
    );

    let dense = |threads| {
        with_threads(threads, || {
            run_dense_gas(TestFaults::default()).metrics.trajectory_hash
        })
    };
    assert_eq!(
        dense(1),
        dense(4),
        "dense gas: 1 thread vs 4 threads diverged"
    );
}

/// Runs a segment right after balls were removed or added. Its step 0 is
/// sampled before the next step rebuilds the cleared or new lists, so it is
/// dropped from the pair history: forces are only computed after the rebuild.
fn run_after_structural_change(main_content: &mut MainContent, steps: usize) -> DriftRunMetrics {
    let mut metrics = run_drift_harness(main_content, hot_gas_config(steps));
    metrics.pair_history.retain(|p| p.step > 0);
    metrics
}

#[test]
#[ignore = "suite:drift"]
fn test_neighbor_lists_survive_balls_removed_and_added() {
    const SEGMENT: usize = 300;
    let mut main_content = hot_gas_world(TestFaults::default());
    let first = run_drift_harness(&mut main_content, hot_gas_config(SEGMENT));

    // Remove every tenth ball through the real removal path (retain, clear
    // all lists, rebuild the cell grid).
    let removed = {
        let balls = &mut main_content.world_mut().balls;
        let mut removed = 0;
        for ball in balls.iter_mut().step_by(10) {
            ball.set_position_and_update_tail(Vec2D::new(f32::NAN, f32::NAN));
            removed += 1;
        }
        removed
    };
    main_content.remove_invalid_balls();
    let after_removal = main_content.world().balls.len();
    let second = run_after_structural_change(&mut main_content, SEGMENT);

    // Add balls at free spots (rejected where they would overlap).
    let factory = scenario_factory();
    let world_rect = main_content.world().world_region();
    let (tl_x, tl_y) = world_rect.get_tl();
    let (br_x, br_y) = world_rect.get_br();
    let mut added = 0;
    for k in 0..60 {
        let t = k as f32 / 60.0;
        let pos = Vec2D::new(
            tl_x + (br_x - tl_x) * (0.05 + 0.9 * (t * 7.3).fract()),
            br_y + (tl_y - br_y) * (0.05 + 0.9 * t),
        );
        let vel = Vec2D::new((k as f32).sin() * HOT_V, (k as f32).cos() * HOT_V);
        let ball = factory.create_massive_with_velocity(pos, vel);
        if main_content.add_candidate_ball(ball, 0.5).is_ok() {
            added += 1;
        }
    }
    let third = run_after_structural_change(&mut main_content, SEGMENT);

    let mut violations = list_violations(&first);
    violations.extend(list_violations(&second));
    violations.extend(list_violations(&third));
    if after_removal != HOT_BALLS - removed {
        violations.push(format!(
            "removal: expected {} balls, found {after_removal}",
            HOT_BALLS - removed
        ));
    }
    if added < 20 {
        violations.push(format!(
            "scenario invalid: only {added} balls could be added"
        ));
    }
    if third.final_sample.ball_count != after_removal + added {
        violations.push(format!(
            "balls lost after adding: expected {}, found {}",
            after_removal + added,
            third.final_sample.ball_count
        ));
    }
    third.report("balls_removed_and_added", violations.is_empty());
    println!(
        "balls_removed_and_added diagnostics: removed={removed} added={added} rebuild_rates={:.3e}/{:.3e}/{:.3e}",
        first.ball_rebuild_rate(),
        second.ball_rebuild_rate(),
        third.ball_rebuild_rate()
    );
    assert!(
        violations.is_empty(),
        "balls removed and added failed:\n  {}",
        violations.join("\n  ")
    );
}
