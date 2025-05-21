use super::*;
use crate::frame::VerletIntegration;
use ggez::GameResult;
use ggez::graphics::Rect;

impl<'a> VerletIntegration for MainContent<'a> {
    fn update_sim(&mut self, dt: f32, bound_rect: &Rect) -> GameResult {
        let mut max_v: f32 = 0.;
        let mut max_id: usize = 0;
        let mut pot: f32 = 0.;
        let mut kin: f32 = 0.;
        let mut _v_com: Vec2D = Vec2D::new(0., 0.);
        let mut sum_mass: f32 = 0.;

        //update positions
        for ball_rc in &mut self.v_points {
            let mut cur = ball_rc.borrow_mut();
            self.physics.update_positions(&mut *cur, dt, bound_rect);
        }
        self.v_points
            .iter()
            .for_each(|ball| ball.borrow_mut().update_old_pos());

        //update velocities and accel with new positions
        self.sim.reset_min_dist(bound_rect);
        for ball_rc in &mut self.v_points {
            let mut cur = ball_rc.borrow_mut();
            let (min_dist, cur_pot) = self
                .physics
                .update_accel_and_speed(&mut *cur, dt, bound_rect);

            //calculate properties
            self.sim.update_min_dist(min_dist);

            pot += cur_pot * 0.5; //double counts
            _v_com += cur.moment();
            let speed_sq = cur.v.norm_sq();
            kin += speed_sq * cur.mass * 0.5;
            sum_mass += cur.mass;
            if max_v * max_v < speed_sq {
                max_v = f32::sqrt(speed_sq);
                max_id = cur.get_id();
            }
        }

        self.sim
            .update(kin, pot, max_id, max_v, dt, _v_com.scaled(1. / sum_mass));
        Ok(())
    }
}
