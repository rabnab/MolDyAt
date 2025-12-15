use super::StatsAccumulator;
use super::*;
use crate::application::VerletIntegration;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::ForceResult;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::simd_forces::force_simd;
use rayon::prelude::*;

impl MainContent {
    pub(crate) fn increase_wait_time(&mut self) {
        self.wait_time = (self.wait_time + 1).min(100);
    }

    pub(crate) fn decrease_wait_time(&mut self) {
        self.wait_time = self.wait_time.saturating_sub(1);
    }

    /// Call this when a window resize event is detected to pause the simulation
    pub(crate) fn pause_for_resize(&mut self) {
        self.was_paused_for_resize = true;
        self.sim.toggle_pause();
        self.resize_resume_at = Some(std::time::Instant::now()); // + std::time::Duration::from_millis(3000));
    }

    /// Call this periodically (e.g., in the update loop) to check if it's time to unpause
    /// the simulation after a resize event.
    pub(crate) fn unpause_after_resize(&mut self) {
        if let Some(resume_time) = self.resize_resume_at
            && std::time::Instant::now() >= resume_time
        {
            if self.was_paused_for_resize && self.sim.is_paused() {
                self.sim.toggle_pause();
            }

            let nu_rect = self.world_region();
            let nu_grid = CellGrid::resize(
                &mut self.cell_grid,
                nu_rect,
                self.physics.get_cutoff_distance(),
                &mut self.balls,
            )
            .expect("Failed to resize CellGrid");
            self.cell_grid = nu_grid;
            self.resize_resume_at = None;
            self.was_paused_for_resize = false;
        }
    }

    /// Update all ball positions and collect bounce information
    fn update_positions(&mut self, dt: f32, bound_rect: &Rect) -> Vec<BounceInfo> {
        let physics = &self.physics;
        let bounce_infos: Vec<BounceInfo> = self
            .balls
            .par_iter_mut()
            .map(|ball| physics.update_positions(ball, dt, bound_rect))
            .collect();

        bounce_infos
    }

    /// Update old positions for all balls
    fn update_old_positions(&mut self) {
        self.balls
            .par_iter_mut()
            .for_each(|ball| ball.update_old_pos());
    }

    /// Compute forces for all balls in parallel with SIMD acceleration.
    fn compute_forces(&self) -> Vec<ForceResult> {
        let physics = &self.physics;
        let balls = &self.balls;
        let results: Vec<ForceResult> = (0..balls.len())
            .into_par_iter()
            .map(|i| force_simd(physics, i, balls))
            .collect();

        results
    }

    /// Apply a single force result to a ball, apply wall bounces, and collect statistics
    fn apply_single_force_and_collect_stats(
        &mut self,
        ball_idx: usize,
        result: &ForceResult,
        bounce_info: &BounceInfo,
        dt: f32,
        speed_limit: f32,
        stats_accumulator: &mut StatsAccumulator,
    ) -> (f32, f32) {
        let ball = &mut self.balls[ball_idx];

        let mut wall_px = 0.0;
        let mut wall_py = 0.0;

        // Compute new acceleration from forces at final position
        let accel = ball.accel_with_speed_limit(
            &result.force,
            self.physics.constants.gravity,
            speed_limit,
            dt,
        );

        if bounce_info.bounced_x || bounce_info.bounced_y {
            // Split timestep: integrate to bounce, apply bounce, integrate remainder
            let t_collision = bounce_info.collision_time_fraction;

            // Integrate velocity from t=0 to t=t_collision (before bounce)
            // Use average of old and new acceleration for the pre-bounce period
            let v_at_collision_x = ball.v.x + 0.5 * (ball.accel.x + accel.x) * dt * t_collision;
            let v_at_collision_y = ball.v.y + 0.5 * (ball.accel.y + accel.y) * dt * t_collision;

            // Wall receives impulse equal to 2 × m × v_perpendicular (elastic bounce)
            // This is the momentum transferred TO the wall
            if bounce_info.bounced_x {
                wall_px = 2.0 * ball.mass * v_at_collision_x;
            }
            if bounce_info.bounced_y {
                wall_py = 2.0 * ball.mass * v_at_collision_y;
            }

            // Apply bounce at collision time (velocity reversal)
            let v_after_bounce_x = if bounce_info.bounced_x {
                -v_at_collision_x
            } else {
                v_at_collision_x
            };
            let v_after_bounce_y = if bounce_info.bounced_y {
                -v_at_collision_y
            } else {
                v_at_collision_y
            };

            // Integrate velocity from t=t_collision to t=dt (after bounce)
            // Continue with same acceleration for remainder of timestep
            ball.v.x = v_after_bounce_x + 0.5 * (ball.accel.x + accel.x) * dt * (1.0 - t_collision);
            ball.v.y = v_after_bounce_y + 0.5 * (ball.accel.y + accel.y) * dt * (1.0 - t_collision);
        } else {
            // No bounce - standard Verlet integration
            ball.v.x += 0.5 * (accel.x + ball.accel.x) * dt;
            ball.v.y += 0.5 * (accel.y + ball.accel.y) * dt;
        }

        ball.accel = accel;

        if ball.verbose.console {
            println!("updated to  v {:?}  a {:?}", ball.v, ball.accel);
        }

        // Accumulate statistics
        self.sim.update_min_dist(result.min_dist);

        stats_accumulator.pot += result.potential_energy * 0.5; // double counts
        stats_accumulator.v_com += ball.moment();
        let speed_sq = ball.v.norm_sq();
        stats_accumulator.kin += speed_sq * ball.mass * 0.5;
        stats_accumulator.sum_mass += ball.mass;
        if stats_accumulator.max_v * stats_accumulator.max_v < speed_sq {
            stats_accumulator.max_v = f32::sqrt(speed_sq);
            stats_accumulator.max_id = ball.get_id();
        }

        (wall_px, wall_py)
    }

    /// Apply force results, process wall bounces, and collect statistics
    fn apply_forces_and_collect_stats(
        &mut self,
        force_results: &[ForceResult],
        bounce_infos: &[BounceInfo],
        dt: f32,
        bound_rect: &Rect,
        stats_accumulator: &mut StatsAccumulator,
    ) -> (f32, f32) {
        let speed_limit = self
            .speed_limit_override
            .unwrap_or_else(|| f32::min(bound_rect.w, bound_rect.h) * 0.2);
        let mut total_wall_px = 0.0;
        let mut total_wall_py = 0.0;

        for (i, result) in force_results.iter().enumerate() {
            let (wall_px, wall_py) = self.apply_single_force_and_collect_stats(
                i,
                result,
                &bounce_infos[i],
                dt,
                speed_limit,
                stats_accumulator,
            );
            total_wall_px += wall_px;
            total_wall_py += wall_py;
        }

        (total_wall_px, total_wall_py)
    }
}

impl VerletIntegration for MainContent {
    fn update_sim(&mut self, dt: f32, bound_rect: &Rect) {
        let mut stats_accumulator = StatsAccumulator {
            pot: 0.,
            kin: 0.,
            v_com: Vec2D::zero(),
            sum_mass: 0.,
            max_v: 0.,
            max_id: 0,
        };

        // Update positions in parallel and collect bounce info
        let bounce_infos = self.update_positions(dt, bound_rect);

        // Update old positions in parallel
        self.update_old_positions();

        // Reset min_dist before velocity/accel updates
        self.sim.reset_min_dist(bound_rect);

        // Compute forces in parallel with SIMD, then apply updates and bounces
        let force_results = self.compute_forces();

        // Cache force results for reuse in rendering (avoid recalculating forces)
        // Clone is necessary because apply_forces_and_collect_stats requires &mut self
        // Trade-off: one-time clone cost << eliminating sequential force recalculations during rendering
        self.cached_force_results = force_results.clone();

        let (wall_px, wall_py) = self.apply_forces_and_collect_stats(
            &force_results,
            &bounce_infos,
            dt,
            bound_rect,
            &mut stats_accumulator,
        );

        // Store wall momentum for tracking
        self.sim.wall_momentum_x += wall_px;
        self.sim.wall_momentum_y += wall_py;

        stats_accumulator.v_com = stats_accumulator
            .v_com
            .scaled(1. / stats_accumulator.sum_mass);

        self.sim.update(dt, stats_accumulator);
    }
}
