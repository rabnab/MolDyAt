// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{DT_GOAL, create_main_content_for_region};
use crate::application::{GameLogic, MainContent};
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{BallFactory, BallId};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy)]
pub struct DriftHarnessConfig {
    pub steps: usize,
    pub sample_every: usize,
    pub dt: f32,
}

impl Default for DriftHarnessConfig {
    fn default() -> Self {
        Self {
            steps: 100,
            sample_every: 10,
            dt: DT_GOAL,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DriftSample {
    pub step: usize,
    pub kinetic: f32,
    pub potential: f32,
    pub total_energy: f32,
    pub momentum: Vec2D,
    pub momentum_scale: f32,
    pub max_overlap_depth: f32,
    pub cap_hit_ratio: f32,
    pub ball_count: usize,
}

/// Pair diagnostics collected after every step (not only on sample steps), so
/// short-lived artifacts such as a stale neighbour list are not missed.
#[derive(Debug, Clone, Copy, Default)]
pub struct PairStepStats {
    pub step: usize,
    /// Pairs within the LJ cutoff that are missing from either neighbour list
    /// (a stale Verlet list silently drops or halves their interaction).
    pub missed_neighbor_pairs: usize,
    /// Pairs listed by only one of the two balls (one-sided force).
    pub asymmetric_neighbor_pairs: usize,
    /// Deepest edge overlap this step as (id_a, id_b, depth).
    pub deepest_overlap: Option<(BallId, BallId, f32)>,
    pub cap_hits: usize,
    pub pair_count: usize,
    pub min_edge_distance: f32,
}

#[derive(Debug)]
pub struct DriftRunMetrics {
    pub initial: DriftSample,
    pub final_sample: DriftSample,
    pub samples: Vec<DriftSample>,
    /// One entry per step, including step 0.
    pub pair_history: Vec<PairStepStats>,
    pub steps: usize,
    pub dt: f32,
    /// Steps in which at least one ball rebuilt its neighbour list.
    pub neighbor_rebuild_count: usize,
    /// Per-ball neighbour-list rebuilds summed over the whole run.
    pub ball_rebuild_events: usize,
    /// Hash of every sample, every step's pair stats and the final positions
    /// and velocities (bit-exact).
    pub trajectory_hash: u64,
}

impl DriftRunMetrics {
    pub fn relative_energy_drift(&self) -> f32 {
        let denom = self.initial.total_energy.abs().max(1e-10);
        (self.final_sample.total_energy - self.initial.total_energy).abs() / denom
    }

    pub fn momentum_drift_magnitude(&self) -> f32 {
        self.final_sample
            .momentum
            .dist(&self.initial.momentum)
            .norm()
    }

    pub fn relative_momentum_drift(&self) -> f32 {
        let denom = self.initial.momentum_scale.max(1e-10);
        self.momentum_drift_magnitude() / denom
    }

    /// Largest relative energy deviation from the initial total over all samples.
    pub fn max_relative_energy_drift(&self) -> f32 {
        let denom = self.initial.total_energy.abs().max(1e-10);
        self.samples
            .iter()
            .map(|s| (s.total_energy - self.initial.total_energy).abs() / denom)
            .fold(0.0f32, f32::max)
    }

    /// Fraction of (ball, step) slots in which a neighbour-list rebuild happened.
    pub fn ball_rebuild_rate(&self) -> f32 {
        let slots = self.initial.ball_count * self.steps;
        if slots == 0 {
            return 0.0;
        }
        self.ball_rebuild_events as f32 / slots as f32
    }

    /// Largest per-step fraction of pairs sitting in the force-cap region.
    pub fn max_cap_hit_ratio(&self) -> f32 {
        self.pair_history
            .iter()
            .filter(|p| p.pair_count > 0)
            .map(|p| p.cap_hits as f32 / p.pair_count as f32)
            .fold(0.0f32, f32::max)
    }

    pub fn total_cap_hits(&self) -> usize {
        self.pair_history.iter().map(|p| p.cap_hits).sum()
    }

    pub fn min_edge_distance(&self) -> f32 {
        self.pair_history
            .iter()
            .map(|p| p.min_edge_distance)
            .fold(f32::INFINITY, f32::min)
    }

    pub fn max_overlap_depth(&self) -> f32 {
        self.pair_history
            .iter()
            .filter_map(|p| p.deepest_overlap.map(|(_, _, d)| d))
            .fold(0.0f32, f32::max)
    }

    pub fn max_missed_neighbor_pairs(&self) -> usize {
        self.pair_history
            .iter()
            .map(|p| p.missed_neighbor_pairs)
            .max()
            .unwrap_or(0)
    }

    pub fn first_missed_neighbor_step(&self) -> Option<usize> {
        self.pair_history
            .iter()
            .find(|p| p.missed_neighbor_pairs > 0)
            .map(|p| p.step)
    }

    pub fn max_asymmetric_neighbor_pairs(&self) -> usize {
        self.pair_history
            .iter()
            .map(|p| p.asymmetric_neighbor_pairs)
            .max()
            .unwrap_or(0)
    }

    pub fn first_asymmetric_neighbor_step(&self) -> Option<usize> {
        self.pair_history
            .iter()
            .find(|p| p.asymmetric_neighbor_pairs > 0)
            .map(|p| p.step)
    }

    /// Longest run of consecutive steps in which the *same* pair is the
    /// deepest overlap and deeper than `depth`: (steps, pair ids).
    pub fn longest_overlap_streak(&self, depth: f32) -> (usize, Option<(BallId, BallId)>) {
        let mut longest = (0usize, None);
        let mut current = 0usize;
        let mut current_pair = None;
        for p in &self.pair_history {
            match p.deepest_overlap {
                Some((a, b, d)) if d > depth => {
                    if current_pair == Some((a, b)) {
                        current += 1;
                    } else {
                        current = 1;
                        current_pair = Some((a, b));
                    }
                    if current > longest.0 {
                        longest = (current, current_pair);
                    }
                }
                _ => {
                    current = 0;
                    current_pair = None;
                }
            }
        }
        longest
    }

    /// One machine-readable summary line per scenario (see `docs/drift-test.md`).
    pub fn report(&self, scenario: &str, passed: bool) {
        println!(
            "DRIFT_SCENARIO name={} steps={} dt={} balls={} final_balls={} max_rel_energy_drift={:.3e} final_rel_energy_drift={:.3e} rel_momentum_drift={:.3e} max_cap_hit_ratio={:.3e} cap_hits={} min_edge_distance={:.3} max_overlap_depth={:.3e} rebuild_steps={} ball_rebuild_rate={:.3e} max_missed_neighbor_pairs={} max_asymmetric_neighbor_pairs={} trajectory_hash={:016x} result={}",
            scenario,
            self.steps,
            self.dt,
            self.initial.ball_count,
            self.final_sample.ball_count,
            self.max_relative_energy_drift(),
            self.relative_energy_drift(),
            self.relative_momentum_drift(),
            self.max_cap_hit_ratio(),
            self.total_cap_hits(),
            self.min_edge_distance(),
            self.max_overlap_depth(),
            self.neighbor_rebuild_count,
            self.ball_rebuild_rate(),
            self.max_missed_neighbor_pairs(),
            self.max_asymmetric_neighbor_pairs(),
            self.trajectory_hash,
            if passed { "pass" } else { "fail" }
        );
    }
}

/// Fraction of a threshold above which a passing metric is flagged, so shrinking
/// margins show up in CI before they turn into failures.
pub const DRIFT_WARNING_FRACTION: f32 = 0.7;

/// Prints a machine-readable `DRIFT_WARNING` line when `value` is above
/// [`DRIFT_WARNING_FRACTION`] of `threshold` (CI turns these into annotations).
pub fn warn_near_threshold(scenario: &str, metric: &str, value: f32, threshold: f32) {
    if threshold > 0.0 && value > DRIFT_WARNING_FRACTION * threshold {
        println!(
            "DRIFT_WARNING name={scenario} metric={metric} value={value:.3e} threshold={threshold:.3e} ratio={:.2}",
            value / threshold
        );
    }
}

/// The drift world: 100 x 100 centred on the origin. Far from the origin f32
/// positions get coarse (~3e-5 at 500), and that rounding, not the
/// integrator, then dominates the energy error (see docs/drift-test.md).
pub fn create_dense_harness_with_seed(seed: u64) -> MainContent {
    let (main_content, _) = create_main_content_for_region(-50.0, 50.0);
    let world_rect = main_content.world().world_region();
    MainContent::new_with_seed(world_rect, seed).expect("Could not create deterministic world")
}

/// Centre of the world region.
pub fn world_center(main_content: &MainContent) -> Vec2D {
    let world = main_content.world().world_region();
    Vec2D::new(world.x + 0.5 * world.w, world.y + 0.5 * world.h)
}

pub fn populate_grid_balls(main_content: &mut MainContent, num_balls: usize, thermal_v: f32) {
    let factory = BallFactory::new();
    let world_rect = main_content.world().world_region();
    let margin = 2.0;
    let (tl_x, tl_y) = world_rect.get_tl();
    let (br_x, br_y) = world_rect.get_br();
    let min_x = tl_x + margin;
    let max_x = br_x - margin;
    let min_y = br_y + margin;
    let max_y = tl_y - margin;

    let grid_size = (num_balls as f32).sqrt().ceil() as usize;
    let x_spacing = (max_x - min_x) / (grid_size as f32);
    let y_spacing = (max_y - min_y) / (grid_size as f32);

    let mut balls_added = 0usize;
    for i in 0..grid_size {
        for j in 0..grid_size {
            if balls_added >= num_balls {
                break;
            }
            let x = min_x + (i as f32 + 0.5) * x_spacing;
            let y = min_y + (j as f32 + 0.5) * y_spacing;
            let pos = Vec2D::new(x, y);
            let vx = (i as f32 * 0.617 + j as f32 * 0.271).sin() * thermal_v;
            let vy = (i as f32 * 0.314 + j as f32 * 0.159).cos() * thermal_v;
            let vel = Vec2D::new(vx, vy);
            let ball = factory.create_massive_with_velocity(pos, vel);
            main_content.world_mut().balls.push(ball);
            balls_added += 1;
        }
    }

    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);
}

pub fn run_drift_harness(
    main_content: &mut MainContent,
    cfg: DriftHarnessConfig,
) -> DriftRunMetrics {
    run_drift_harness_with_observer(main_content, cfg, |_, _| {})
}

/// Like [`run_drift_harness`], but calls `observe(step, main_content)` after
/// every simulation step so scenarios can track per-step events (e.g. wall hits).
pub fn run_drift_harness_with_observer(
    main_content: &mut MainContent,
    cfg: DriftHarnessConfig,
    mut observe: impl FnMut(usize, &MainContent),
) -> DriftRunMetrics {
    let mut samples = Vec::new();
    let mut pair_history = Vec::with_capacity(cfg.steps + 1);
    let mut neighbor_rebuild_count = 0usize;
    let mut ball_rebuild_events = 0usize;
    let initial = sample_step(main_content, 0);
    samples.push(initial);
    pair_history.push(pair_step_stats(main_content, 0));

    // Track last_update_hood_pos to detect when neighbour lists were rebuilt.
    let mut prev_last_update: Vec<_> = main_content
        .world()
        .balls
        .iter()
        .map(|b| b.last_update_hood_pos)
        .collect();

    for step in 1..=cfg.steps {
        // Use the real simulation_step orchestration with caller-provided dt so the
        // harness matches real behaviour while supporting timestep studies.
        // (neighbour updates, position updates, cell-grid moves, removals, and step_count).
        main_content
            .simulation_step_with_dt(cfg.dt)
            .expect("simulation_step_with_dt failed in harness");

        // Detect if any ball's last_update_hood_pos changed -> neighbour rebuild occurred
        let balls = &main_content.world().balls;
        let mut any_changed = false;
        if balls.len() != prev_last_update.len() {
            any_changed = true;
        } else {
            for (i, ball) in balls.iter().enumerate() {
                let prev = prev_last_update[i];
                let curr = ball.last_update_hood_pos;
                if prev.x != curr.x || prev.y != curr.y {
                    any_changed = true;
                    ball_rebuild_events += 1;
                }
            }
        }
        if any_changed {
            neighbor_rebuild_count += 1;
        }

        prev_last_update = main_content
            .world()
            .balls
            .iter()
            .map(|b| b.last_update_hood_pos)
            .collect();

        pair_history.push(pair_step_stats(main_content, step));
        observe(step, main_content);

        let should_sample =
            step == cfg.steps || (cfg.sample_every != 0 && step % cfg.sample_every == 0);
        if should_sample {
            samples.push(sample_step(main_content, step));
        }
    }

    let final_sample = *samples.last().expect("at least one sample");
    let trajectory_hash = trajectory_hash(main_content, &samples, &pair_history);
    DriftRunMetrics {
        initial,
        final_sample,
        samples,
        pair_history,
        steps: cfg.steps,
        dt: cfg.dt,
        neighbor_rebuild_count,
        ball_rebuild_events,
        trajectory_hash,
    }
}

fn trajectory_hash(
    main_content: &MainContent,
    samples: &[DriftSample],
    pair_history: &[PairStepStats],
) -> u64 {
    let mut h = DefaultHasher::new();
    for s in samples {
        s.step.hash(&mut h);
        for v in [
            s.kinetic,
            s.potential,
            s.total_energy,
            s.momentum.x,
            s.momentum.y,
            s.max_overlap_depth,
        ] {
            v.to_bits().hash(&mut h);
        }
        s.ball_count.hash(&mut h);
    }
    for p in pair_history {
        p.missed_neighbor_pairs.hash(&mut h);
        p.asymmetric_neighbor_pairs.hash(&mut h);
        p.cap_hits.hash(&mut h);
        p.min_edge_distance.to_bits().hash(&mut h);
    }
    for b in &main_content.world().balls {
        let pos = b.get_position();
        for v in [pos.x, pos.y, b.v.x, b.v.y] {
            v.to_bits().hash(&mut h);
        }
    }
    h.finish()
}

fn pair_step_stats(main_content: &MainContent, step: usize) -> PairStepStats {
    let world = main_content.world();
    let balls = &world.balls;
    let lj_cutoff = world.physics.constants.lj_cutoff;
    let max_force = world.physics.constants.max_force;
    let mut stats = PairStepStats {
        step,
        min_edge_distance: f32::INFINITY,
        ..Default::default()
    };

    for i in 0..balls.len() {
        for j in (i + 1)..balls.len() {
            let center_dist = balls[j].get_position().dist(balls[i].get_position()).norm();
            let edge_dist = center_dist - (balls[i].radius + balls[j].radius);
            stats.pair_count += 1;
            stats.min_edge_distance = stats.min_edge_distance.min(edge_dist);

            let i_lists_j = balls[i].hood.is_a_neighbour_idx(j);
            let j_lists_i = balls[j].hood.is_a_neighbour_idx(i);
            if i_lists_j != j_lists_i {
                stats.asymmetric_neighbor_pairs += 1;
            }
            if edge_dist < lj_cutoff && !(i_lists_j && j_lists_i) {
                stats.missed_neighbor_pairs += 1;
            }

            if edge_dist < 0.0
                && stats
                    .deepest_overlap
                    .is_none_or(|(_, _, depth)| -edge_dist > depth)
            {
                stats.deepest_overlap = Some((balls[i].get_id(), balls[j].get_id(), -edge_dist));
            }
            if max_force.is_finite() && world.physics.force(edge_dist).abs() >= max_force * 0.999 {
                stats.cap_hits += 1;
            }
        }
    }
    stats
}

fn sample_step(main_content: &MainContent, step: usize) -> DriftSample {
    let world = main_content.world();
    let mut kinetic = 0.0f32;
    let mut potential = 0.0f32;
    let mut momentum = Vec2D::new(0.0, 0.0);
    let mut momentum_scale = 0.0f32;
    let mut max_overlap_depth = 0.0f32;
    let mut cap_hits = 0usize;
    let mut pair_count = 0usize;
    let max_force = world.physics.constants.max_force;

    for ball in &world.balls {
        let speed_sq = ball.v.x * ball.v.x + ball.v.y * ball.v.y;
        let speed = speed_sq.sqrt();
        kinetic += 0.5 * ball.mass * speed_sq;
        momentum.x += ball.mass * ball.v.x;
        momentum.y += ball.mass * ball.v.y;
        momentum_scale += ball.mass * speed;
    }

    for i in 0..world.balls.len() {
        for j in (i + 1)..world.balls.len() {
            let dist_vec = world.balls[j]
                .get_position()
                .dist(world.balls[i].get_position());
            let center_dist = dist_vec.norm();
            let edge_dist = center_dist - (world.balls[i].radius + world.balls[j].radius);
            potential += world.physics.potential(edge_dist);
            pair_count += 1;
            if edge_dist < 0.0 {
                max_overlap_depth = max_overlap_depth.max(-edge_dist);
            }
            if max_force.is_finite() {
                let force_mag = world.physics.force(edge_dist).abs();
                if force_mag >= max_force * 0.999 {
                    cap_hits += 1;
                }
            }
        }
    }

    let cap_hit_ratio = if pair_count == 0 {
        0.0
    } else {
        cap_hits as f32 / pair_count as f32
    };

    DriftSample {
        step,
        kinetic,
        potential,
        total_energy: kinetic + potential,
        momentum,
        momentum_scale,
        max_overlap_depth,
        cap_hit_ratio,
        ball_count: world.balls.len(),
    }
}

#[cfg(test)]
mod dense_gas;
#[cfg(test)]
mod neighbor_lists;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wall_reflection;
