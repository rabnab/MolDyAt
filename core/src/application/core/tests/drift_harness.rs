use super::{DT_GOAL, create_main_content_for_region};
use crate::application::{GameLogic, MainContent};
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::BallFactory;

mod tests;

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
    pub max_overlap_depth: f32,
    pub cap_hit_ratio: f32,
}

#[derive(Debug)]
pub struct DriftRunMetrics {
    pub initial: DriftSample,
    pub final_sample: DriftSample,
    pub samples: Vec<DriftSample>,
    pub neighbor_rebuild_count: usize,
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
}

pub fn create_dense_harness_with_seed(seed: u64) -> MainContent {
    let (main_content, _) = create_main_content_for_region(500.0, 500.0);
    let world_rect = main_content.world().world_region();
    MainContent::new_with_seed(world_rect, seed).expect("Could not create deterministic world")
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
    let mut samples = Vec::new();
    let mut neighbor_rebuild_count = 0usize;
    let initial = sample_step(main_content, 0);
    samples.push(initial);

    // Track last_update_hood_pos to detect when neighbour lists were rebuilt.
    let mut prev_last_update: Vec<_> = main_content
        .world()
        .balls
        .iter()
        .map(|b| b.last_update_hood_pos)
        .collect();

    for step in 1..=cfg.steps {
        // Use the real simulation_step orchestration so the harness matches real behaviour
        // (neighbour updates, position updates, cell-grid moves, removals, and step_count).
        main_content
            .simulation_step()
            .expect("simulation_step failed in harness");

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
                    break;
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

        let should_sample =
            step == cfg.steps || (cfg.sample_every != 0 && step % cfg.sample_every == 0);
        if should_sample {
            samples.push(sample_step(main_content, step));
        }
    }

    let final_sample = *samples.last().expect("at least one sample");
    DriftRunMetrics {
        initial,
        final_sample,
        samples,
        neighbor_rebuild_count,
    }
}

fn sample_step(main_content: &MainContent, step: usize) -> DriftSample {
    let world = main_content.world();
    let mut kinetic = 0.0f32;
    let mut potential = 0.0f32;
    let mut momentum = Vec2D::new(0.0, 0.0);
    let mut max_overlap_depth = 0.0f32;
    let mut cap_hits = 0usize;
    let mut pair_count = 0usize;
    let max_force = world.physics.constants.max_force;

    for ball in &world.balls {
        let speed_sq = ball.v.x * ball.v.x + ball.v.y * ball.v.y;
        kinetic += 0.5 * ball.mass * speed_sq;
        momentum.x += ball.mass * ball.v.x;
        momentum.y += ball.mass * ball.v.y;
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
        max_overlap_depth,
        cap_hit_ratio,
    }
}
