use super::StatsAccumulator;
use super::*;
use crate::frame::VerletIntegration;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::ForceResult;
use crate::objects2d::ball_impl::simd_forces::force_indexed_simd;
use rayon::prelude::*;

impl MainContent {
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

    /// Update all ball positions
    fn update_positions(&mut self, dt: f32, bound_rect: &Rect) {
        let physics = &self.physics;
        self.balls
            .par_iter_mut()
            .for_each(|ball| physics.update_positions(ball, dt, bound_rect));
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
            .map(|i| force_indexed_simd(physics, i, balls))
            .collect();

        results
    }

    /// Apply a single force result to a ball and collect statistics
    fn apply_single_force_and_collect_stats(
        &mut self,
        ball_idx: usize,
        result: &ForceResult,
        dt: f32,
        speed_limit: f32,
        stats_accumulator: &mut StatsAccumulator,
    ) {
        let ball = &mut self.balls[ball_idx];
        let accel = ball.accel_with_speed_limit(
            &result.force,
            self.physics.constants.gravity,
            speed_limit,
            dt,
        );
        ball.v.x += 0.5 * (accel.x + ball.accel.x) * dt;
        ball.v.y += 0.5 * (accel.y + ball.accel.y) * dt;
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
    }

    /// Apply force results and collect statistics
    fn apply_forces_and_collect_stats(
        &mut self,
        force_results: &[ForceResult],
        dt: f32,
        bound_rect: &Rect,
        stats_accumulator: &mut StatsAccumulator,
    ) {
        let speed_limit = f32::min(bound_rect.w, bound_rect.h) * 0.2;
        for (i, result) in force_results.iter().enumerate() {
            self.apply_single_force_and_collect_stats(
                i,
                result,
                dt,
                speed_limit,
                stats_accumulator,
            );
        }
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

        // Update positions in parallel
        self.update_positions(dt, bound_rect);

        // Update old positions in parallel
        self.update_old_positions();

        // Reset min_dist before velocity/accel updates
        self.sim.reset_min_dist(bound_rect);

        // Compute forces in parallel with SIMD, then apply updates
        let force_results = self.compute_forces();
        self.apply_forces_and_collect_stats(&force_results, dt, bound_rect, &mut stats_accumulator);

        stats_accumulator.v_com = stats_accumulator
            .v_com
            .scaled(1. / stats_accumulator.sum_mass);

        self.sim.update(dt, stats_accumulator);
    }
}
