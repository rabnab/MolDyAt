use super::*;
use crate::frame::VerletIntegration;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::ForceResult;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

#[cfg(feature = "simd")]
use crate::objects2d::ball_impl::simd_forces::force_indexed_simd;

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
        #[cfg(feature = "parallel")]
        {
            let physics = &self.physics;
            self.balls
                .par_iter_mut()
                .for_each(|ball| physics.update_positions(ball, dt, bound_rect));
        }
        #[cfg(not(feature = "parallel"))]
        {
            for ball in &mut self.balls {
                self.physics.update_positions(ball, dt, bound_rect);
            }
        }
    }

    /// Update old positions for all balls
    fn update_old_positions(&mut self) {
        #[cfg(feature = "parallel")]
        {
            self.balls
                .par_iter_mut()
                .for_each(|ball| ball.update_old_pos());
        }
        #[cfg(not(feature = "parallel"))]
        {
            for ball in &mut self.balls {
                ball.update_old_pos();
            }
        }
    }

    /// Compute forces for all balls in parallel.
    ///
    /// # Feature requirements
    /// - This method is only available when the `parallel` feature is enabled.
    /// - If the `simd` feature is also enabled, SIMD acceleration will be used for force computation.
    #[cfg(feature = "parallel")]
    fn compute_forces_parallel(&self) -> Vec<ForceResult> {
        #[cfg(feature = "simd")]
        {
            let physics = &self.physics;
            let balls = &self.balls;
            (0..balls.len())
                .into_par_iter()
                .map(|i| force_indexed_simd(physics, i, balls))
                .collect()
        }
        #[cfg(not(feature = "simd"))]
        {
            let physics = &self.physics;
            let balls = &self.balls;
            (0..balls.len())
                .into_par_iter()
                .map(|i| physics.force_indexed(i, balls))
                .collect()
        }
    }

    /// Apply force results and collect statistics
    fn apply_forces_and_collect_stats(
        &mut self,
        force_results: &[ForceResult],
        dt: f32,
        bound_rect: &Rect,
        pot: &mut f32,
        kin: &mut f32,
        _v_com: &mut Vec2D,
        sum_mass: &mut f32,
        max_v: &mut f32,
        max_id: &mut usize,
    ) {
        let speed_limit = f32::min(bound_rect.w, bound_rect.h) * 0.2;
        for (i, result) in force_results.iter().enumerate() {
            let ball = &mut self.balls[i];
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
            *pot += result.potential_energy * 0.5; // double counts
            *_v_com += ball.moment();
            let speed_sq = ball.v.norm_sq();
            *kin += speed_sq * ball.mass * 0.5;
            *sum_mass += ball.mass;
            if *max_v * *max_v < speed_sq {
                *max_v = f32::sqrt(speed_sq);
                *max_id = ball.get_id();
            }
        }
    }

    /// Update velocities and accelerations sequentially with SIMD forces
    #[cfg(all(not(feature = "parallel"), feature = "simd"))]
    fn update_velocities_simd_sequential(
        &mut self,
        dt: f32,
        bound_rect: &Rect,
        pot: &mut f32,
        kin: &mut f32,
        _v_com: &mut Vec2D,
        sum_mass: &mut f32,
        max_v: &mut f32,
        max_id: &mut usize,
    ) {
        let speed_limit = f32::min(bound_rect.w, bound_rect.h) * 0.2;
        for i in 0..self.balls.len() {
            let result = force_indexed_simd(&self.physics, i, &self.balls);

            let ball = &mut self.balls[i];
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
            *pot += result.potential_energy * 0.5; // double counts
            *_v_com += ball.moment();
            let speed_sq = ball.v.norm_sq();
            *kin += speed_sq * ball.mass * 0.5;
            *sum_mass += ball.mass;
            if *max_v * *max_v < speed_sq {
                *max_v = f32::sqrt(speed_sq);
                *max_id = ball.get_id();
            }
        }
    }

    /// Update velocities and accelerations sequentially without SIMD
    #[cfg(all(not(feature = "parallel"), not(feature = "simd")))]
    fn update_velocities_scalar_sequential(
        &mut self,
        dt: f32,
        bound_rect: &Rect,
        pot: &mut f32,
        kin: &mut f32,
        _v_com: &mut Vec2D,
        sum_mass: &mut f32,
        max_v: &mut f32,
        max_id: &mut usize,
    ) {
        for i in 0..self.balls.len() {
            let (min_dist, cur_pot) =
                self.physics
                    .update_accel_and_speed_indexed(i, &mut self.balls, dt, bound_rect);

            //calculate properties
            self.sim.update_min_dist(min_dist);

            *pot += cur_pot * 0.5; //double counts
            *_v_com += self.balls[i].moment();
            let speed_sq = self.balls[i].v.norm_sq();
            *kin += speed_sq * self.balls[i].mass * 0.5;
            *sum_mass += self.balls[i].mass;
            if *max_v * *max_v < speed_sq {
                *max_v = f32::sqrt(speed_sq);
                *max_id = self.balls[i].get_id();
            }
        }
    }
}

impl VerletIntegration for MainContent {
    fn update_sim(&mut self, dt: f32, bound_rect: &Rect) {
        let mut max_v: f32 = 0.;
        let mut max_id: usize = 0;
        let mut pot: f32 = 0.;
        let mut kin: f32 = 0.;
        let mut _v_com: Vec2D = Vec2D::new(0., 0.);
        let mut sum_mass: f32 = 0.;

        // Update positions - can be parallelized as each ball is independent
        self.update_positions(dt, bound_rect);

        // Update old positions - can be parallelized
        self.update_old_positions();

        // Reset min_dist before velocity/accel updates
        self.sim.reset_min_dist(bound_rect);

        // Update velocities and acceleration with new positions
        #[cfg(feature = "parallel")]
        {
            // Parallel version: compute forces in parallel, then apply updates
            let force_results = self.compute_forces_parallel();
            self.apply_forces_and_collect_stats(
                &force_results,
                dt,
                bound_rect,
                &mut pot,
                &mut kin,
                &mut _v_com,
                &mut sum_mass,
                &mut max_v,
                &mut max_id,
            );
        }
        #[cfg(not(feature = "parallel"))]
        {
            // Sequential version
            #[cfg(feature = "simd")]
            self.update_velocities_simd_sequential(
                dt,
                bound_rect,
                &mut pot,
                &mut kin,
                &mut _v_com,
                &mut sum_mass,
                &mut max_v,
                &mut max_id,
            );

            #[cfg(not(feature = "simd"))]
            self.update_velocities_scalar_sequential(
                dt,
                bound_rect,
                &mut pot,
                &mut kin,
                &mut _v_com,
                &mut sum_mass,
                &mut max_v,
                &mut max_id,
            );
        }

        self.sim
            .update(kin, pot, max_id, max_v, dt, _v_com.scaled(1. / sum_mass));
    }
}
