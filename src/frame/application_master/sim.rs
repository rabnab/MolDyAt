use super::*;
use crate::frame::VerletIntegration;
use crate::objects2d::Rect;

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
        if let Some(resume_time) = self.resize_resume_at {
            if std::time::Instant::now() >= resume_time {
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

        //update positions
        for ball in &mut self.balls {
            self.physics.update_positions(ball, dt, bound_rect);
        }
        for ball in &mut self.balls {
            ball.update_old_pos();
        }

        //update velocities and accel with new positions
        self.sim.reset_min_dist(bound_rect);

        for i in 0..self.balls.len() {
            let (min_dist, cur_pot) =
                self.physics
                    .update_accel_and_speed_indexed(i, &mut self.balls, dt, bound_rect);

            //calculate properties
            self.sim.update_min_dist(min_dist);

            pot += cur_pot * 0.5; //double counts
            _v_com += self.balls[i].moment();
            let speed_sq = self.balls[i].v.norm_sq();
            kin += speed_sq * self.balls[i].mass * 0.5;
            sum_mass += self.balls[i].mass;
            if max_v * max_v < speed_sq {
                max_v = f32::sqrt(speed_sq);
                max_id = self.balls[i].get_id();
            }
        }

        self.sim
            .update(kin, pot, max_id, max_v, dt, _v_com.scaled(1. / sum_mass));
    }
}
