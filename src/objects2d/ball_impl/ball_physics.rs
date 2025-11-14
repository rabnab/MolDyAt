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
            max_force: 100.0,
            //softening: 0.01,
            softening: 0.0,
            lj_cutoff: LJ_CUTOFF, // Standard cutoff for L-J potential 2.5
        }
    }
}

/// Result of force calculation for a ball, including minimum distance to neighbors,
/// total force vector, and total potential energy from interactions.
#[derive(Clone, Debug)]
pub struct ForceResult {
    /// Minimum edge-to-edge distance to the nearest neighbor
    pub min_dist: f32,
    /// Total force vector acting on the ball from all neighbors
    pub force: Vec2D,
    /// Total potential energy from all neighbor interactions
    pub potential_energy: f32,
}

#[derive(Clone)]
pub struct BallPhysics {
    pub constants: PhysicsConstants,
    /// Cached distance at which LJ force equals max_force
    r_cap: f32,
}

impl Default for BallPhysics {
    fn default() -> Self {
        let constants = PhysicsConstants::default();
        let r_cap = Self::calculate_force_cap_distance(&constants);
        BallPhysics { constants, r_cap }
    }
}

impl BallPhysics {
    pub fn new(constants: PhysicsConstants) -> Self {
        let r_cap = Self::calculate_force_cap_distance(&constants);
        BallPhysics { constants, r_cap }
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

    /// Calculate Lennard-Jones potential energy with force capping.
    ///
    /// The standard Lennard-Jones potential is:
    ///   U(r) = 4·ε·[(σ/r)^12 - (σ/r)^6]
    ///
    /// At equilibrium (r = 2^(1/6)·σ), U = -ε (minimum potential energy).
    ///
    /// # Force Capping for Energy Conservation
    /// When r < r_cap (where force would exceed max_force), we use a linear potential:
    ///   U(r) = U(r_cap) + max_force·(r - r_cap)
    ///
    /// This linear extrapolation ensures:
    /// 1. Energy continuity at r = r_cap: U(r_cap⁻) = U(r_cap⁺)
    /// 2. Force continuity: F = -dU/dr = max_force for r < r_cap
    /// 3. Conservative force field (energy is conserved)
    ///
    /// # Arguments
    /// * `dist_btw_balls` - Edge-to-edge distance between ball surfaces
    pub(crate) fn lenard_jones_potential(&self, dist_btw_balls: f32) -> f32 {
        let r_cap = self.get_force_cap_distance();

        if dist_btw_balls < r_cap {
            // Force capping region: use linear potential to preserve energy
            let quot = self.constants.lj_sig / r_cap; // σ/r_cap
            let quot_six = quot.powi(6); // (σ/r_cap)^6

            // Calculate potential at the cap distance
            let pot_at_cap = 4. * self.constants.lj_eps * quot_six * (quot_six - 1.);

            // Linear extrapolation: U(r) = U(r_cap) + F_cap·(r - r_cap)
            // Since F = -dU/dr, and F_cap is repulsive (positive),
            // potential increases as r decreases below r_cap
            return pot_at_cap + self.constants.max_force * (dist_btw_balls - r_cap);
        }

        let quot = self.constants.lj_sig / dist_btw_balls;
        let quot_six = quot.powi(6);
        4. * self.constants.lj_eps * quot_six * (quot_six - 1.)
    }

    /// Calculate the distance r_cap at which the Lennard-Jones force equals max_force.
    ///
    /// This is critical for energy conservation when force capping is enabled. We solve:
    ///   |F_LJ(r_cap)| = max_force
    ///
    /// Where the Lennard-Jones force is:
    ///   F_LJ(r) = 24·ε·(σ/r)^6 · [2·(σ/r)^6 - 1] / r
    ///
    /// # Energy Conservation Strategy
    /// For r < r_cap, we cap the force at max_force and use a linear potential:
    ///   U(r) = U(r_cap) + max_force·(r - r_cap)
    /// This ensures energy continuity at r_cap and maintains conservative forces.
    ///
    /// # Algorithm
    /// 1. Try Newton's method for fast convergence (typically 5-10 iterations)
    /// 2. Fall back to binary search if Newton's method fails to converge
    ///
    /// # Returns
    /// The distance r_cap where |F_LJ(r_cap)| = max_force
    fn calculate_force_cap_distance(constants: &PhysicsConstants) -> f32 {
        const NEWTON_MAX_ITERATIONS: usize = 20;
        const NEWTON_CONVERGENCE_THRESHOLD: f32 = 1e-6;
        const INITIAL_GUESS_FACTOR: f32 = 0.5; // Start at 0.5·σ

        let max_force = constants.max_force;
        let lj_sig = constants.lj_sig;
        let lj_eps = constants.lj_eps;

        // Initial guess: start at σ/2, which is typically in the steep repulsive region
        let mut r = lj_sig * INITIAL_GUESS_FACTOR;

        // Newton's method: r_new = r - f(r)/f'(r)
        // where f(r) = F_LJ(r) - max_force
        // F_LJ(r) = 24·ε·(σ/r)^6 · [2·(σ/r)^6 - 1] / r
        for iteration in 0..NEWTON_MAX_ITERATIONS {
            let quot = lj_sig / r; // σ/r
            let quot_six = quot.powi(6); // (σ/r)^6
            let quot_twelve = quot_six * quot_six; // (σ/r)^12

            // Calculate Lennard-Jones force magnitude:
            // F(r) = 24·ε·(σ/r)^6 · [2·(σ/r)^6 - 1] / r
            let force = 24.0 * lj_eps * quot_six * (2.0 * quot_six - 1.0) / r;

            // Calculate derivative dF/dr using chain rule:
            // dF/dr = -24·ε·[14·(σ/r)^12 - 7·(σ/r)^6] / r²
            let force_derivative = -24.0 * lj_eps * (14.0 * quot_twelve - 7.0 * quot_six) / (r * r);

            // Newton step: update r to move closer to the root
            let delta = (force - max_force) / force_derivative;
            r -= delta;

            // Check convergence: stop when change is < 0.0001% of r
            if delta.abs() < NEWTON_CONVERGENCE_THRESHOLD * r {
                if cfg!(test) || iteration < 5 {
                    // Newton's method typically converges in 3-5 iterations for good initial guess
                }
                return r;
            }
        }

        // Fallback: Binary search if Newton's method didn't converge
        // This is rare but provides robustness for extreme parameter values
        const BINARY_SEARCH_MAX_ITERATIONS: usize = 50;
        const BINARY_SEARCH_TOLERANCE: f32 = 0.001; // 0.1% tolerance
        const SEARCH_RANGE_LOW_FACTOR: f32 = 0.1; // Search from 0.1·σ
        const SEARCH_RANGE_HIGH_FACTOR: f32 = 1.0; // Search up to 1.0·σ

        let mut r_low = lj_sig * SEARCH_RANGE_LOW_FACTOR;
        let mut r_high = lj_sig * SEARCH_RANGE_HIGH_FACTOR; // Force root is always below σ

        for _ in 0..BINARY_SEARCH_MAX_ITERATIONS {
            let r_mid = (r_low + r_high) / 2.0;
            let quot = lj_sig / r_mid;
            let quot_six = quot.powi(6);
            let force = (24.0 * lj_eps * quot_six * (2.0 * quot_six - 1.0) / r_mid).abs();

            // Check if we've found a good enough solution
            if (force - max_force).abs() < BINARY_SEARCH_TOLERANCE * max_force {
                return r_mid;
            }

            // Bisect: force decreases with increasing r
            if force > max_force {
                r_low = r_mid; // Force too high, need larger r
            } else {
                r_high = r_mid; // Force too low, need smaller r
            }
        }

        // Return best estimate even if not fully converged
        (r_low + r_high) / 2.0
    }

    /// Get the cached force cap distance
    pub(crate) fn get_force_cap_distance(&self) -> f32 {
        self.r_cap
    }

    pub(crate) fn lenard_jones_force(
        &self,
        dist_of_balls: f32,
        radius_sum: f32,
        _id: usize,
    ) -> f32 {
        let dist = dist_of_balls - radius_sum;
        let r_cap = self.get_force_cap_distance();

        if dist < r_cap {
            // Force is constant at max_force below cap distance
            self.constants.max_force
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
        let result = self.force_indexed(ball_idx, balls);
        let speed_limit = f32::min(bound.w, bound.h) * 0.2;

        let ball = &mut balls[ball_idx];
        let accel =
            ball.accel_with_speed_limit(&result.force, self.constants.gravity, speed_limit, dt);
        ball.v.x += 0.5 * (accel.x + ball.accel.x) * dt;
        ball.v.y += 0.5 * (accel.y + ball.accel.y) * dt;
        ball.accel = accel;

        if ball.verbose.console {
            println!("updated to  v {:?}  a {:?}", ball.v, ball.accel);
        }
        (result.min_dist, result.potential_energy)
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

    pub(crate) fn force_indexed(&self, ball_idx: usize, balls: &[Ball2D]) -> ForceResult {
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
        ForceResult {
            min_dist,
            force,
            potential_energy: total_pot,
        }
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

    #[test]
    fn test_force_cap_at_max_force() {
        let phys = BallPhysics::default();
        let max_force = phys.constants.max_force;
        let r_cap = phys.get_force_cap_distance();

        // At r_cap, force should be approximately max_force
        let force_at_cap = phys.lenard_jones_force_pure(r_cap);
        approx(force_at_cap, max_force, 1e-2);

        // Below r_cap, force should be capped at max_force
        let r_below_cap = r_cap * 0.5;
        let force_below = phys.lenard_jones_force(r_below_cap, 0.0, 0);
        approx(force_below, max_force, 1e-6);

        // Well below r_cap, force should still be capped
        let r_very_small = r_cap * 0.1;
        let force_very_small = phys.lenard_jones_force(r_very_small, 0.0, 0);
        approx(force_very_small, max_force, 1e-6);
    }

    #[test]
    fn test_potential_linear_below_cap() {
        let phys = BallPhysics::default();
        let max_force = phys.constants.max_force;
        let r_cap = phys.get_force_cap_distance();

        // Get potential at r_cap
        let pot_at_cap = phys.lenard_jones_potential(r_cap);

        // Check potential at a distance below r_cap
        let r_below = r_cap * 0.8;
        let pot_below = phys.lenard_jones_potential(r_below);

        // Expected potential: U(r) = U(r_cap) + max_force * (r - r_cap)
        let expected_pot = pot_at_cap + max_force * (r_below - r_cap);
        approx(pot_below, expected_pot, 1e-4);

        // Check another point
        let r_below2 = r_cap * 0.5;
        let pot_below2 = phys.lenard_jones_potential(r_below2);
        let expected_pot2 = pot_at_cap + max_force * (r_below2 - r_cap);
        approx(pot_below2, expected_pot2, 1e-4);
    }

    #[test]
    fn test_force_not_capped_above_threshold() {
        let phys = BallPhysics::default();
        let max_force = phys.constants.max_force;
        let r_cap = phys.get_force_cap_distance();

        // Above r_cap, force should follow normal LJ formula
        let r_above = r_cap * 2.0;
        let force_above = phys.lenard_jones_force(r_above, 0.0, 0);
        let force_pure = phys.lenard_jones_force_pure(r_above);

        // Force should be less than max_force
        assert!(
            force_above < max_force,
            "Force above cap distance should be less than max_force"
        );
        // Force should match pure LJ formula
        approx(force_above, force_pure, 1e-6);
    }

    #[test]
    fn test_energy_conservation_with_force_cap() {
        // Test that energy is conserved when force capping is active
        let phys = BallPhysics::default();
        let r_cap = phys.get_force_cap_distance();
        let max_force = phys.constants.max_force;

        // Verify force is properly capped at r_cap
        let force_at_cap = phys.lenard_jones_force_pure(r_cap);
        approx(force_at_cap, max_force, 1.0); // Within 1 unit (1% for max_force=100)

        // Test linear potential below r_cap
        let pot_at_cap = phys.lenard_jones_potential(r_cap);
        let r_below1 = r_cap * 0.9;
        let r_below2 = r_cap * 0.7;

        let pot_below1 = phys.lenard_jones_potential(r_below1);
        let pot_below2 = phys.lenard_jones_potential(r_below2);

        // Check linearity: slope should be max_force
        // U(r) = U(r_cap) + max_force * (r - r_cap)
        let expected_pot1 = pot_at_cap + max_force * (r_below1 - r_cap);
        let expected_pot2 = pot_at_cap + max_force * (r_below2 - r_cap);

        approx(pot_below1, expected_pot1, 1e-4);
        approx(pot_below2, expected_pot2, 1e-4);

        // Verify the slope between two points in the linear region
        let delta_r = r_below1 - r_below2;
        let delta_u = pot_below1 - pot_below2;
        let slope = delta_u / delta_r;

        // Slope should equal max_force (since F = -dU/dr and F is constant)
        approx(slope, max_force, 0.1);
    }

    #[test]
    fn test_force_cap_calculation_robustness() {
        // Test that r_cap calculation works for various parameter combinations
        let test_cases = vec![
            (1.0, 1.0, 50.0),  // Standard parameters, lower max_force
            (1.0, 1.0, 100.0), // Default parameters
            (1.0, 1.0, 200.0), // Higher max_force
            (0.5, 1.0, 100.0), // Smaller sigma
            (2.0, 1.0, 100.0), // Larger sigma
            (1.0, 0.5, 100.0), // Weaker epsilon
            (1.0, 2.0, 100.0), // Stronger epsilon
        ];

        for (lj_sig, lj_eps, max_force) in test_cases {
            let constants = PhysicsConstants {
                lj_sig,
                lj_eps,
                max_force,
                ..PhysicsConstants::default()
            };

            let phys = BallPhysics::new(constants);
            let r_cap = phys.get_force_cap_distance();

            // r_cap should be positive and less than sigma
            assert!(
                r_cap > 0.0,
                "r_cap must be positive for sig={}, eps={}, F_max={}",
                lj_sig,
                lj_eps,
                max_force
            );
            assert!(
                r_cap < lj_sig,
                "r_cap should be less than sigma for sig={}, eps={}, F_max={}",
                lj_sig,
                lj_eps,
                max_force
            );

            // Force at r_cap should approximately equal max_force
            let force_at_cap = phys.lenard_jones_force_pure(r_cap);
            let rel_error = (force_at_cap - max_force).abs() / max_force;
            assert!(
                rel_error < 0.01, // 1% tolerance
                "Force at r_cap should equal max_force: got {}, expected {}, error = {}%",
                force_at_cap,
                max_force,
                rel_error * 100.0
            );
        }
    }
}
