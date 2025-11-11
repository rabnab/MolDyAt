use crate::objects2d::Rect;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{Ball2D, BallError, BallResult, Named};

//const LJ_SIG: f32 = 5.; // equilibrium distance
//LJ_SIG in SI units is for Argon 3.4A=3.4E-10m
const LJ_SIG: f32 = 1.; // equilibrium distance
//pub(crate) const LJ_CUTOFF: f32 = 8. * LJ_SIG; //distance where lenard jones force is < 1e-4 (as long as sig>0.1)
const LJ_CUTOFF: f32 = 4. * LJ_SIG; //distance where lenard jones force is < 1e-4 (as long as sig>0.1)
const LJ_EPS: f32 = 1.; // attractive strength
const SQRT6_OF_TWO: f32 = 1.122_462; // == 2^(1/6)

#[derive(Clone)]
pub struct PhysicsConstants {
    pub lj_sig: f32,
    pub lj_eps: f32,
    pub lj_cutoff: f32,
    pub gravity: f32,
    pub damping: f32,
    pub softening: f32,
    pub max_force: f32,
}

impl Default for PhysicsConstants {
    fn default() -> Self {
        PhysicsConstants {
            lj_eps: LJ_EPS,
            lj_sig: LJ_SIG,
            //gravity: 9.81,
            gravity: 0.0,
            //gravity: 1.0,
            damping: 1.0,
            max_force: 1000.0,
            //softening: 0.01,
            softening: 0.0,
            lj_cutoff: LJ_CUTOFF, // Standard cutoff for L-J potential 2.5
        }
    }
}
#[derive(Clone, Default)]
pub struct BallPhysics {
    pub constants: PhysicsConstants,
}

impl BallPhysics {
    pub fn new(constants: PhysicsConstants) -> Self {
        BallPhysics { constants }
    }

    /// the cutoff distance used for evaluating lenard jones potentail
    pub fn get_cutoff_distance(&self) -> f32 {
        self.constants.lj_cutoff
    }

    /// Returns the minimum movement threshold required before a ball's update hood (region for neighbor search)
    /// needs to be recalculated. This threshold is set to half the difference between the cell size and the cutoff
    /// distance, i.e., `(cell_size - cutoff) / 2.0`, which is equivalent to `0.75 * cutoff` when `cell_size = 2.5 * cutoff`.
    /// The cell size (`2.5 * cutoff`) is chosen to ensure that neighbor searches are efficient and accurate.
    /// By only updating the hood when a ball moves more than this threshold, unnecessary recalculations are avoided,
    /// improving performance while maintaining correctness in neighbor detection.
    ///
    /// Note: The max_radius parameter accounts for the largest ball in the simulation. Since cutoff is measured
    /// edge-to-edge but ball movement is center-to-center, we need to consider the ball radii when determining
    /// if a neighbor might have entered or left the interaction range.
    pub fn get_min_movement_for_update_hood(&self, max_radius: f32) -> f32 {
        let cutoff = self.get_cutoff_distance();
        // The cell size must accommodate the cutoff plus the maximum ball diameter
        // to ensure all potential neighbors are captured
        let effective_cutoff = cutoff + 2.0 * max_radius;
        let cell_size = 2.5 * effective_cutoff;
        (cell_size - effective_cutoff) / 2.0
    }

    /// equilibrium distance of two balls with same radius
    pub(crate) fn equilibrium_of_radius(&self, radius: f32) -> f32 {
        //case: r1==r2 --- SQRT6_OF_TWO * LJ_SIG + 2* rad == dist_equi
        (SQRT6_OF_TWO * self.constants.lj_sig) * 0.5 + radius
    }

    pub(crate) fn lenard_jones_potential(&self, dist_btw_balls: f32) -> f32 {
        //case dist_of_balls = equilibrium_radius*2 and radius_sum=2*radius
        //dist = SQRT6_OF_TWO LJ_SIG + 2*rad - 2*rad
        //dist = SQRT6_OF_TWO LJ_SIG
        //quot = LJ_SIG/dist = 1/SQRT6_OF_TWO
        //quot_six = 1/2
        //potential: 4 LJ_EPS 1/2 *(1/2-1)
        //    4 LJ_EPS 1/2 * (-1/2)
        //    - LJ_EPS

        if dist_btw_balls < self.min_dist_for_const_force() {
            let quot = self.constants.lj_sig / self.min_dist_for_const_force();
            let quot_six = quot.powi(6);
            let pot_at_min_dist = 4. * self.constants.lj_eps * quot_six * (quot_six - 1.);

            let force_at_min_dist = self.lenard_jones_force_pure(self.min_dist_for_const_force());
            return pot_at_min_dist
                + (dist_btw_balls - self.min_dist_for_const_force()) * force_at_min_dist;
        }

        let quot = self.constants.lj_sig / dist_btw_balls;
        let quot_six = quot.powi(6);
        4. * self.constants.lj_eps * quot_six * (quot_six - 1.)
    }

    fn min_dist_for_const_force(&self) -> f32 {
        self.constants.lj_sig * 0.75
    }

    pub(crate) fn lenard_jones_force(
        &self,
        dist_of_balls: f32,
        radius_sum: f32,
        _id: usize,
    ) -> f32 {
        let dist = dist_of_balls - radius_sum;
        if dist < self.min_dist_for_const_force() {
            self.lenard_jones_force_pure(self.min_dist_for_const_force())
        } else {
            self.lenard_jones_force_pure(dist)
        }
    }
    pub(crate) fn lenard_jones_force_pure(&self, dist_btw_balls: f32) -> f32 {
        let quot = self.constants.lj_sig / dist_btw_balls;
        let quot_six = quot.powi(6);

        24. * self.constants.lj_eps * quot_six * (2. * quot_six - 1.) / dist_btw_balls
    }

    pub(crate) fn calculate_min_dist(actual_dist: f32, min_dist: f32) -> BallResult<f32> {
        // let wiggle_room = LJ_SIG*0.1;
        // if actual_dist < 0. {
        //     return Err(BallError::LessThanTwoBalls);
        // }
        if f32::is_nan(min_dist) {
            if actual_dist <= 0. {
                return Err(BallError::DistanceUnderflow);
            }
            return Ok(actual_dist);
        }
        if actual_dist <= 0. {
            return Err(BallError::DistanceUnderflow);
        }
        Ok(f32::min(min_dist, actual_dist))
    }

    pub fn distance_of_edges(a: &Ball2D, b: &Ball2D) -> f32 {
        let center_dist = a.get_position().dist(b.get_position()).norm();
        center_dist - (a.radius + b.radius)
    }

    pub fn is_colliding(&self, this: &Ball2D, other: &Ball2D) -> bool {
        //use norm_sq to avoid sqrt in norm
        let radius_sum_sq = (this.radius + other.radius/*+ self.constants.lj_sig*/).powi(2);
        let res = this.get_position().dist(other.get_position()).norm_sq() < radius_sum_sq;
        if res {
            println!(
                "ball {} at {} is too close to {}: dist = {} sum_rad = {}",
                this.id,
                this.get_position(),
                other.get_position(),
                this.get_position().dist(other.get_position()).norm(),
                this.radius + other.radius
            )
        }
        res
    }

    pub fn update_positions(&self, ball: &mut Ball2D, dt: f32, bound: &Rect) {
        if ball.verbose.console {
            println!(
                "{} ------ [{:.2},{:.2}]",
                ball.get_name(),
                ball.hood.nearest,
                ball.hood.farthest
            );
        }

        let dt_sub = dt;
        let int_steps: usize = 1;
        ball.execute_update_pos(dt_sub, bound);

        if ball.verbose.console {
            println!(
                "{} --- dt: {} substeps: {} new pos: {} new v: {}",
                ball.get_name(),
                dt,
                int_steps,
                ball.get_position(),
                ball.v
            );
        }
        //TODO check if it is true that this should only be done later
        //self.update_old_pos();
    }

    pub fn get_neighbour_force_and_pos(
        &self,
        ball: &Ball2D,
        balls: &[Ball2D],
    ) -> Vec<(Vec2D, f32)> {
        let ball_pos = ball.get_position().clone();
        let ball_radius = ball.radius;
        ball.hood
            .get_neighbour_indices()
            .iter()
            .filter_map(|&idx| balls.get(idx))
            .map(|other| {
                (
                    other.get_position().clone(),
                    self.lenard_jones_force(
                        ball_pos.dist(other.get_position()).norm(),
                        ball_radius + other.radius,
                        other.get_id(),
                    ),
                )
            })
            .collect()
    }

    pub fn update_accel_and_speed_indexed(
        &self,
        ball_idx: usize,
        balls: &mut [Ball2D],
        dt: f32,
        bound: &Rect,
    ) -> (f32, f32) {
        let (min_dist, total_force, total_pot) = self.force_indexed(ball_idx, balls);
        let speed_limit = f32::min(bound.w, bound.h) * 0.2;

        let ball = &mut balls[ball_idx];
        let accel =
            ball.accel_with_speed_limit(&total_force, self.constants.gravity, speed_limit, dt);
        ball.v.x += 0.5 * (accel.x + ball.accel.x) * dt;
        ball.v.y += 0.5 * (accel.y + ball.accel.y) * dt;
        ball.accel = accel;

        if ball.verbose.console {
            println!("updated to  v {:?}  a {:?}", ball.v, ball.accel);
        }
        (min_dist, total_pot)
    }
    // ball.hood
    //         .neighbours
    //         .borrow()
    //         .iter()
    //         .filter_map(|weak| weak.upgrade()) // Filter out dropped references
    //         .map(|rcrefcell| {
    //             let other = rcrefcell.borrow();
    //             (
    //                 other.get_position().clone(),
    //                 self.lenard_jones_force(
    //                     ball_pos.dist(&other.get_position()).norm(),
    //                     ball_radius + other.radius,
    //                     other.get_id(),
    //                 ),
    //             )
    //         })
    //         .collect()
    // }

    //fn force_by(&self, others: &Vec<Ball2D>)  -> (f32, Vec2D, f32) {

    pub(crate) fn force_indexed(&self, ball_idx: usize, balls: &[Ball2D]) -> (f32, Vec2D, f32) {
        let mut force = Vec2D::new(0., 0.);
        let mut min_dist = f32::NAN;
        let mut total_pot = 0.;

        let ball_pos = balls[ball_idx].get_position().clone();
        let ball_id = balls[ball_idx].id;
        let ball_radius = balls[ball_idx].radius;
        let ball_verbose_console = balls[ball_idx].verbose.console;
        let neighbour_indices = balls[ball_idx].hood.get_neighbour_indices().to_vec();

        for &neighbour_idx in &neighbour_indices {
            if let Some(b) = balls.get(neighbour_idx) {
                //from other_ball to this_ball
                let dist_vec = b.pre_update_pos.dist(&ball_pos);
                let center_dist = dist_vec.norm();
                let radius_sum = b.radius + ball_radius;
                let edge_dist = center_dist - radius_sum;

                match Self::calculate_min_dist(center_dist - radius_sum, min_dist) {
                    Ok(min_dist_cur) => {
                        min_dist = min_dist_cur;
                        let lj_force = self.lenard_jones_force(center_dist, radius_sum, ball_id);
                        let force_to_dist_val = lj_force / center_dist;
                        let local_force = Vec2D::new(
                            dist_vec.x * force_to_dist_val,
                            dist_vec.y * force_to_dist_val,
                        );

                        if ball_verbose_console || edge_dist < 0. {
                            println!(
                                "\t\t {}--force: {} ({},{}) at dist: {}",
                                b.get_name(),
                                local_force.norm(),
                                local_force.x,
                                local_force.y,
                                edge_dist
                            );
                        }

                        force += local_force;
                        total_pot += self.lenard_jones_potential(center_dist - radius_sum);
                    }
                    _ => {
                        min_dist = 0.;
                        //maybe use some max_force to repell
                        let lj_force = 100.;
                        let force_to_dist_val = lj_force / center_dist;
                        let local_force = Vec2D::new(
                            dist_vec.x * force_to_dist_val,
                            dist_vec.y * force_to_dist_val,
                        );

                        if ball_verbose_console || edge_dist < 0. {
                            println!(
                                "\t\t ball {}--force(max): {:?} at dist: {}",
                                b.get_name(),
                                local_force,
                                edge_dist
                            );
                        }

                        force += local_force;
                        total_pot += self.lenard_jones_potential(center_dist - radius_sum);
                    }
                };
            }
        }
        (min_dist, force, total_pot)
    }

    // pub fn clear_hood(&mut self) {
    //     self.hood.neighbours.borrow_mut().clear();
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) {
        assert!(
            (a - b).abs() <= eps * (1.0 + a.abs().max(b.abs())),
            "values not approx equal: {} vs {} with eps {}",
            a,
            b,
            eps
        );
    }

    #[test]
    fn test_lj_equilibrium() {
        let phys = BallPhysics::default();
        // Using default sigma=1 and eps=1 per file constants
        let r_eq = 1.122462048309372981_f32; // 2^(1/6)
        let pot = phys.lenard_jones_potential(r_eq);

        approx(pot, -1.0, 1e-5);
        let force = phys.lenard_jones_force_pure(r_eq);

        approx(force, 0.0, 1e-5);
    }

    #[test]
    fn test_check_min_dist() {
        // When actual <= 0 -> DistanceUnderflow
        assert_eq!(
            BallPhysics::calculate_min_dist(0.0, 1.0).unwrap_err(),
            BallError::DistanceUnderflow
        );
        // When actual < min -> returns actual
        let val = BallPhysics::calculate_min_dist(0.5, 1.0).unwrap();
        approx(val, 0.5, 1e-6);
        // When actual >= min -> returns min
        let val2 = BallPhysics::calculate_min_dist(2.0, 1.0).unwrap();
        approx(val2, 1.0, 1e-6);
    }

    #[test]
    fn test_is_colliding_detection() {
        let phys = BallPhysics::default();
        let a = Ball2D::new(1, Vec2D::new(2.0, 3.0), (0, 4.0));

        let b_pos = Vec2D::new(2.0 + LJ_SIG * 0.99999, 3.0);
        let b = Ball2D::new(2, b_pos, (0, 6.0));

        assert!(phys.is_colliding(&a, &b));

        let c_radius = 6.0;
        let c_pos = Vec2D::new(2.0, 3.0 + (a.radius + c_radius) + LJ_SIG * 1.00001);
        let c = Ball2D::new(2, c_pos, (0, c_radius));

        assert!(!phys.is_colliding(&a, &c));
    }

    #[test]
    fn test_lenard_jones_force_when_close() {
        let phys = BallPhysics::default();
        let r_eq = 1.122462048309372981_f32; // 2^(1/6) * sigma with sigma=1
        // Slightly below equilibrium -> repulsive -> positive force magnitude returned by pure
        let f_near = phys.lenard_jones_force_pure(r_eq * 0.9);
        // Slightly above equilibrium -> attractive -> negative force
        for i in -30..30 {
            let r = i as f32 * r_eq * 0.1;
            let f = phys.lenard_jones_force_pure(r);
            let f2 = phys.lenard_jones_force(r, 0.0, 0);
            println!("r {r}  force: {f} eff: {f2}");
        }

        let f_far = phys.lenard_jones_force_pure(r_eq * 3.0);
        assert!(
            f_near > 0.0,
            "Expected repulsive (positive) force when nearer than equilibrium, got {}",
            f_near
        );
        assert!(
            f_far < 0.0,
            "Expected attractive (negative) force when farther than equilibrium, got {}",
            f_far
        );
    }

    #[test]
    fn test_min_movement_threshold_formula() {
        let phys = BallPhysics::default();
        let cutoff = phys.get_cutoff_distance();
        let max_radius = 1.0; // Example max radius
        let effective_cutoff = cutoff + 2.0 * max_radius;
        let cell_size = 2.5 * effective_cutoff;
        let expected_threshold = (cell_size - effective_cutoff) / 2.0;

        let actual_threshold = phys.get_min_movement_for_update_hood(max_radius);

        approx(actual_threshold, expected_threshold, 1e-6);
        // Also verify the calculation directly: (2.5 * rc_eff - rc_eff) / 2 = 0.75 * rc_eff
        approx(actual_threshold, 0.75 * effective_cutoff, 1e-6);
    }
}
