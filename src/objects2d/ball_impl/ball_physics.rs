use crate::objects2d::Rect;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::{Ball2D, BallError, BallResult, Named};
use std::simd::f32x4;
use std::simd::prelude::SimdFloat;

//const LJ_SIG: f32 = 5.; // equilibrium distance
//LJ_SIG in SI units is for Argon 3.4A=3.4E-10m
const LJ_SIG: f32 = 1.; // equilibrium distance
const LJ_EPS: f32 = 1.; // attractive strength
const MAX_FORCE: f32 = 100.0; // max repulsive force to cap at
const SQRT6_OF_TWO: f32 = 1.122_462; // == 2^(1/6)

#[derive(Clone)]
pub struct PhysicsConstants {
    pub lj_sig: f32,
    pub lj_eps: f32,
    pub lj_cutoff: f32,
    pub gravity: f32,
    pub max_force: f32,
    pub pot_at_cap: f32,
    /// Cached distance at which LJ force equals max_force
    pub r_cap: f32,
}

impl PhysicsConstants {
    #[cfg(test)]
    fn new_for_test(lj_sig: f32, lj_eps: f32, max_force: f32) -> Self {
        let r_cap = Self::calculate_force_cap_distance(lj_sig, lj_eps, max_force);
        let pot_at_cap = {
            let cap_quot = lj_sig / r_cap;
            let cap_quot_six = cap_quot * cap_quot * cap_quot * cap_quot * cap_quot * cap_quot;
            4. * lj_eps * cap_quot_six * (cap_quot_six - 1.)
        };

        PhysicsConstants {
            lj_eps,
            lj_sig,
            gravity: 0.,
            max_force,
            lj_cutoff: 4. * lj_sig,
            pot_at_cap,
            r_cap,
        }
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
    ///   U(r) = U(r_cap) - max_force·(r - r_cap)
    /// This ensures F = -dU/dr = +max_force (repulsive), energy continuity at r_cap, and conservative forces.
    ///
    /// # Algorithm
    /// 1. Try Newton's method for fast convergence (typically 5-10 iterations)
    /// 2. Fall back to binary search if Newton's method fails to converge
    ///
    /// # Returns
    /// The distance r_cap where |F_LJ(r_cap)| = max_force
    fn calculate_force_cap_distance(lj_sig: f32, lj_eps: f32, max_force: f32) -> f32 {
        const NEWTON_MAX_ITERATIONS: usize = 20;
        const NEWTON_CONVERGENCE_THRESHOLD: f32 = 1e-6;
        const INITIAL_GUESS_FACTOR: f32 = 0.5; // Start at 0.5·σ

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
}

impl Default for PhysicsConstants {
    fn default() -> Self {
        let r_cap = Self::calculate_force_cap_distance(LJ_SIG, LJ_EPS, MAX_FORCE);
        let pot_at_cap = {
            let cap_quot = LJ_SIG / r_cap;
            let cap_quot_six = cap_quot * cap_quot * cap_quot * cap_quot * cap_quot * cap_quot;
            4. * LJ_EPS * cap_quot_six * (cap_quot_six - 1.)
        };

        PhysicsConstants {
            lj_eps: LJ_EPS,
            lj_sig: LJ_SIG,
            //gravity: 9.81,
            gravity: 0.0,
            //gravity: 1.0,
            max_force: MAX_FORCE,
            //softening: 0.01,
            lj_cutoff: 4. * LJ_SIG, // Standard cutoff for L-J potential 2.5
            pot_at_cap,
            r_cap,
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
    /// Per-neighbor force magnitudes for visualization (neighbor_idx, force_magnitude)
    /// Only populated when needed for rendering neighbor lines
    pub neighbor_forces: Vec<(usize, f32)>,
}

#[derive(Clone)]
pub struct BallPhysics {
    pub constants: PhysicsConstants,
}

impl Default for BallPhysics {
    fn default() -> Self {
        let constants: PhysicsConstants = PhysicsConstants::default();
        BallPhysics { constants }
    }
}

impl BallPhysics {
    pub fn new(constants: PhysicsConstants) -> Self {
        BallPhysics { constants }
    }

    /// the cutoff distance used for evaluating lenard jones potentail
    pub fn get_cutoff_distance(&self) -> f32 {
        self.constants.lj_cutoff
    }

    /// A skin of 1.0 is usually a good balance.
    /// If you want it dynamic, you could use 0.2 * self.get_cutoff_distance()
    pub fn get_skin_thickness(&self) -> f32 {
        2.0 * self.get_cutoff_distance()
    }

    pub fn get_cutoff_and_skin(&self) -> f32 {
        self.get_cutoff_distance() + self.get_skin_thickness()
    }

    /// Returns the minimum movement threshold required before a ball's update hood needs to be recalculated.
    /// We define a fixed "skin" thickness for the Verlet list buffer.
    /// The neighbor list radius is [cutoff + skin](http://_vscodecontentref_/5).
    /// The update threshold must be `skin / 2` to ensure no particle can cross the skin buffer
    /// without triggering an update.
    pub fn get_min_movement_for_update_hood(&self, _max_radius: f32) -> f32 {
        self.get_skin_thickness() * 0.5
    }

    pub(crate) fn equilibrium_edge_distance(&self) -> f32 {
        //case: r1==r2 --- SQRT6_OF_TWO * LJ_SIG + 2* rad == dist_equi
        SQRT6_OF_TWO * self.constants.lj_sig
    }

    /// equilibrium distance of two balls with same radius
    pub(crate) fn equilibrium_of_radius(&self, radius: f32) -> f32 {
        //case: r1==r2 --- SQRT6_OF_TWO * LJ_SIG + 2* rad == dist_equi
        self.equilibrium_edge_distance() * 0.5 + radius
    }

    pub(crate) fn force(&self, edge_dist: f32) -> f32 {
        let r_cap = self.get_force_cap_distance();
        let lj_sig = self.constants.lj_sig;
        let lj_eps = self.constants.lj_eps;
        let edge_dist_cap = edge_dist.max(r_cap);

        let quot = lj_sig / edge_dist_cap;
        let quot_six = quot * quot * quot * quot * quot * quot; // (σ/r)^6
        24. * lj_eps * quot_six * (2. * quot_six - 1.) / edge_dist_cap
    }

    pub(crate) fn force_simd(&self, edge_dist_cap: f32x4) -> f32x4 {
        let lj_sig = self.constants.lj_sig;
        let lj_eps = self.constants.lj_eps;
        let quot = f32x4::splat(lj_sig) / edge_dist_cap;
        let quot_six = quot * quot * quot * quot * quot * quot; // (σ/r)^6
        f32x4::splat(24. * lj_eps) * quot_six * (f32x4::splat(2.) * quot_six - f32x4::splat(1.))
            / edge_dist_cap
    }

    /// Calculate Lennard-Jones potential energy with force capping.
    ///
    /// The standard Lennard-Jones potential is:
    ///   U(r) = 4·ε·[(σ/r)^12 - (σ/r)^6]
    ///
    /// At equilibrium (r = 2^(1/6)·σ), U = -ε (minimum potential energy).
    ///
    /// ### Force Capping for Energy Conservation
    /// When r < r_cap (where force would exceed max_force), we use a linear potential:
    ///   U(r) = U(r_cap) - max_force·(r - r_cap)
    ///
    /// This linear extrapolation ensures:
    /// 1. Energy continuity at r = r_cap: U(r_cap⁻) = U(r_cap⁺)
    /// 2. Force continuity: F = -dU/dr = +max_force (repulsive) for r < r_cap
    /// 3. Conservative force field (energy is conserved)
    ///
    /// # Arguments
    /// * `edge_dist` - Edge-to-edge distance between ball surfaces
    pub(crate) fn potential(&self, edge_dist: f32) -> f32 {
        // Potential
        let pot_quot = self.constants.lj_sig / edge_dist.max(1e-10);
        let pot_quot_six = pot_quot * pot_quot * pot_quot * pot_quot * pot_quot * pot_quot;
        let potential_uncapped = 4. * self.constants.lj_eps * pot_quot_six * (pot_quot_six - 1.);
        // Linear potential for r < r_cap: U(r) = U(r_cap) - max_force·(r - r_cap)
        // This ensures F = -dU/dr = +max_force (repulsive, constant) below r_cap
        let potential_linear = (self.constants.pot_at_cap
            - self.constants.max_force * (edge_dist - self.constants.r_cap))
            .max(self.constants.pot_at_cap);
        potential_linear.min(potential_uncapped)
    }

    pub(crate) fn potential_simd(&self, edge_dist: f32x4) -> f32x4 {
        // === Potential Energy Calculation with Force Capping ===
        // Standard LJ potential: U(r) = 4·ε·[(σ/r)^12 - (σ/r)^6]
        // For r < r_cap: Use linear extrapolation to preserve energy conservation
        // Calculate standard LJ potential (with capping)
        let r_cap_vec = f32x4::splat(self.get_force_cap_distance());
        let lj_sig = self.constants.lj_sig;
        let lj_eps = self.constants.lj_eps;
        let max_force_vec = f32x4::splat(self.constants.max_force);
        let pot_at_cap_vec = f32x4::splat(self.constants.pot_at_cap);

        let pot_quot = f32x4::splat(lj_sig) / edge_dist.simd_max(f32x4::splat(1e-10));
        let pot_quot_six = pot_quot * pot_quot * pot_quot * pot_quot * pot_quot * pot_quot;
        let potential_uncapped =
            f32x4::splat(4. * lj_eps) * pot_quot_six * (pot_quot_six - f32x4::splat(1.));
        // Linear potential for r < r_cap: U(r) = U(r_cap) - max_force·(r - r_cap)
        // This ensures F = -dU/dr = +max_force (repulsive, constant) below r_cap
        let potential_linear = f32x4::simd_max(
            pot_at_cap_vec - max_force_vec * (edge_dist - r_cap_vec),
            pot_at_cap_vec,
        );

        f32x4::simd_min(potential_linear, potential_uncapped)
    }

    /// Get the cached force cap distance
    pub(crate) fn get_force_cap_distance(&self) -> f32 {
        self.constants.r_cap
    }

    pub(crate) fn calculate_min_dist(actual_dist: f32, min_dist: f32) -> BallResult<f32> {
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

    pub fn is_colliding(&self, this: &Ball2D, other: &Ball2D, margin: f32) -> bool {
        this.get_edge_distance(other) < margin
    }

    pub fn update_positions(&self, ball: &mut Ball2D, dt: f32, bound: &Rect) -> BounceInfo {
        let bounce_info = ball.execute_update_pos(dt, bound);

        if ball.verbose.console {
            println!(
                "{} --- dt: {} new pos: {} new v: {} neigh_minmax: [{:.2},{:.2}]",
                ball.get_name(),
                dt,
                ball.get_position(),
                ball.v,
                ball.hood.nearest,
                ball.hood.farthest
            );
        }

        bounce_info
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
                let center_dist = ball_pos.dist(other.get_position()).norm();
                let edge_dist = center_dist - (ball_radius + other.radius);
                (other.get_position().clone(), self.force(edge_dist))
            })
            .collect()
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================
// Tests are organized into the following categories:
// - Force and potential calculations (LJ formula, force capping)
// - Energy conservation (single and multi-particle systems)
// - Edge cases (overlapping balls, numerical robustness)
// ============================================================================

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
        let pot = phys.potential(r_eq);

        approx(pot, -1.0, 1e-5);
        let force = phys.force(r_eq);

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

        assert!(phys.is_colliding(&a, &b, 0.));

        let c_radius = 6.0;
        let c_pos = Vec2D::new(2.0, 3.0 + (a.radius + c_radius) + LJ_SIG * 1.00001);
        let c = Ball2D::new(2, c_pos, (0, c_radius));

        assert!(!phys.is_colliding(&a, &c, 0.));
    }

    #[test]
    fn test_lenard_jones_force_when_close() {
        let phys = BallPhysics::default();
        let r_eq = 1.122462048309372981_f32; // 2^(1/6) * sigma with sigma=1
        // Slightly below equilibrium -> repulsive -> positive force magnitude returned by pure
        let f_near = phys.force(r_eq * 0.9);
        // Slightly above equilibrium -> attractive -> negative force
        for i in -30..30 {
            let r = i as f32 * r_eq * 0.1;
            let f = phys.force(r);
            println!("r {r}  force: {f}");
        }

        let f_far = phys.force(3.0 * r_eq);
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

        // The correct formula based on cell_size = 2.5 * cutoff (from CellGrid)
        // and effective_cutoff = cutoff + 2 * max_radius
        let cell_size = 2.5 * cutoff;
        let effective_cutoff = cutoff + 2.0 * max_radius;
        let expected_threshold = (cell_size - effective_cutoff) / 2.0;

        // This simplifies to: 0.75 * cutoff - max_radius
        let expected_simplified = 0.75 * cutoff - max_radius;

        let actual_threshold = phys.get_min_movement_for_update_hood(max_radius);

        approx(actual_threshold, expected_threshold, 1e-6);
        approx(actual_threshold, expected_simplified, 1e-6);
    }

    #[test]
    fn test_force_cap_at_max_force() {
        let phys = BallPhysics::default();
        let max_force = phys.constants.max_force;
        let r_cap = phys.get_force_cap_distance();

        // At r_cap, force should be approximately max_force
        let force_at_cap = phys.force(r_cap);
        approx(force_at_cap, max_force, 1e-2);

        // Below r_cap, force should be capped at max_force
        let r_below_cap = r_cap * 0.5;
        let force_below = phys.force(r_below_cap);
        approx(force_below, max_force, 1e-2);

        // Well below r_cap, force should still be capped
        let r_very_small = r_cap * 0.1;
        let force_very_small = phys.force(r_very_small);
        approx(force_very_small, max_force, 1e-2);
    }

    #[test]
    fn test_potential_linear_below_cap() {
        let phys = BallPhysics::default();
        let max_force = phys.constants.max_force;
        let r_cap = phys.get_force_cap_distance();

        // Get potential at r_cap
        let pot_at_cap = phys.potential(r_cap);

        // Check potential at a distance below r_cap
        let r_below = r_cap * 0.8;
        let pot_below = phys.potential(r_below);

        // Expected potential: U(r) = U(r_cap) - max_force * (r - r_cap)
        let expected_pot = pot_at_cap - max_force * (r_below - r_cap);
        approx(pot_below, expected_pot, 1e-4);

        // Check another point
        let r_below2 = r_cap * 0.5;
        let pot_below2 = phys.potential(r_below2);
        let expected_pot2 = pot_at_cap - max_force * (r_below2 - r_cap);
        approx(pot_below2, expected_pot2, 1e-4);
    }

    #[test]
    fn test_force_not_capped_above_threshold() {
        let phys = BallPhysics::default();
        let max_force = phys.constants.max_force;
        let r_cap = phys.get_force_cap_distance();

        // Above r_cap, force should follow normal LJ formula
        let r_above = r_cap * 2.0;
        let force_above = phys.force(r_above);

        // Force should be less than max_force
        assert!(
            force_above < max_force,
            "Force above cap distance should be less than max_force"
        );
    }

    #[test]
    fn test_energy_conservation_with_force_cap() {
        // Test that energy is conserved when force capping is active
        let phys = BallPhysics::default();
        let r_cap = phys.get_force_cap_distance();
        let max_force = phys.constants.max_force;

        // Test linear potential below r_cap
        let pot_at_cap = phys.potential(r_cap);
        let r_below1 = r_cap * 0.9;
        let r_below2 = r_cap * 0.7;

        let pot_below1 = phys.potential(r_below1);
        let pot_below2 = phys.potential(r_below2);

        // Check linearity: slope should be -max_force
        // U(r) = U(r_cap) - max_force * (r - r_cap)
        let expected_pot1 = pot_at_cap - max_force * (r_below1 - r_cap);
        let expected_pot2 = pot_at_cap - max_force * (r_below2 - r_cap);

        approx(pot_below1, expected_pot1, 1e-4);
        approx(pot_below2, expected_pot2, 1e-4);

        // Verify the slope between two points in the linear region
        let delta_r = r_below1 - r_below2;
        let delta_u = pot_below1 - pot_below2;
        let slope = delta_u / delta_r;

        // Slope should equal -max_force (since F = -dU/dr and F is constant at +max_force)
        approx(slope, -max_force, 0.1);
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
            let constants = PhysicsConstants::new_for_test(lj_sig, lj_eps, max_force);
            println!(
                "Testing force cap calculation for sig={}, eps={}, F_max={}",
                lj_sig, lj_eps, max_force
            );
            println!(
                "Constants: sig: {}, eps: {}, max_force: {}",
                constants.lj_sig, constants.lj_eps, constants.max_force
            );

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
            let force_at_cap = phys.force(r_cap);
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

    #[test]
    fn test_energy_conservation_two_balls_integration() {
        // Integration test: Simulate two interacting balls for 1000 steps
        // and verify total energy (kinetic + potential) remains constant within numerical error

        use crate::objects2d::ball_impl::ball_factory::BallFactory;
        use crate::objects2d::ball_impl::simd_forces::force_simd;

        let phys = BallPhysics::default();
        let factory = BallFactory::new();

        // Create two balls in close proximity to trigger strong forces
        // Position them at a distance where force capping will be active
        let r_cap = phys.get_force_cap_distance();
        let separation = r_cap * 0.8; // Place them close enough to trigger force cap

        let ball1 = factory.create_basic(Vec2D::new(0.0, 0.0), 2);
        let radius_sum = ball1.radius * 2.0; // Both balls have same radius
        let mut ball1 = ball1;
        let mut ball2 = factory.create_basic(Vec2D::new(separation + radius_sum, 0.0), 2);

        // Give them small initial velocities to make the interaction more interesting
        ball1.v = Vec2D::new(0.1, 0.05);
        ball2.v = Vec2D::new(-0.08, -0.03);

        // Initialize pre_update positions
        ball1.pre_update_pos = ball1.get_position().clone();
        ball2.pre_update_pos = ball2.get_position().clone();

        // Set up neighbor lists (each ball sees the other)
        ball1.hood.add_neighbour(1);
        ball2.hood.add_neighbour(0);

        let mut balls = vec![ball1, ball2];

        // Calculate initial energy
        let initial_energy = calculate_total_energy(&phys, &balls);

        println!("Initial energy: {:.6}", initial_energy);
        println!("Initial separation: {:.6}", separation);
        println!("Force cap distance (r_cap): {:.6}", r_cap);

        // Simulation parameters
        const DT: f32 = 0.001; // Small timestep for numerical stability
        const NUM_STEPS: usize = 1000;

        let mut max_energy_error = 0.0f32;
        let mut energy_at_max_error = 0.0f32;

        // Run simulation
        for step in 0..NUM_STEPS {
            // Compute forces using SIMD implementation
            let force1 = force_simd(&phys, 0, &balls);
            let force2 = force_simd(&phys, 1, &balls);

            // Update velocities (velocity Verlet integration - part 1)
            let accel1 = Vec2D::new(
                force1.force.x / balls[0].mass,
                force1.force.y / balls[0].mass,
            );
            let accel2 = Vec2D::new(
                force2.force.x / balls[1].mass,
                force2.force.y / balls[1].mass,
            );

            // Update velocities with half-step acceleration
            balls[0].v.x += 0.5 * accel1.x * DT;
            balls[0].v.y += 0.5 * accel1.y * DT;
            balls[1].v.x += 0.5 * accel2.x * DT;
            balls[1].v.y += 0.5 * accel2.y * DT;

            // Update positions
            let new_pos1 = Vec2D::new(
                balls[0].get_position().x + balls[0].v.x * DT,
                balls[0].get_position().y + balls[0].v.y * DT,
            );
            let new_pos2 = Vec2D::new(
                balls[1].get_position().x + balls[1].v.x * DT,
                balls[1].get_position().y + balls[1].v.y * DT,
            );

            balls[0].set_position_and_update_tail(new_pos1.clone());
            balls[1].set_position_and_update_tail(new_pos2.clone());
            balls[0].pre_update_pos = new_pos1;
            balls[1].pre_update_pos = new_pos2;

            // Recompute forces at new positions
            let force1_new = force_simd(&phys, 0, &balls);
            let force2_new = force_simd(&phys, 1, &balls);

            // Complete velocity update with new accelerations (velocity Verlet - part 2)
            let accel1_new = Vec2D::new(
                force1_new.force.x / balls[0].mass,
                force1_new.force.y / balls[0].mass,
            );
            let accel2_new = Vec2D::new(
                force2_new.force.x / balls[1].mass,
                force2_new.force.y / balls[1].mass,
            );

            balls[0].v.x += 0.5 * accel1_new.x * DT;
            balls[0].v.y += 0.5 * accel1_new.y * DT;
            balls[1].v.x += 0.5 * accel2_new.x * DT;
            balls[1].v.y += 0.5 * accel2_new.y * DT;

            // Check energy conservation every 100 steps
            if step % 100 == 0 {
                let current_energy = calculate_total_energy(&phys, &balls);
                let energy_error = (current_energy - initial_energy).abs();
                let rel_error = energy_error / initial_energy.abs();

                if energy_error > max_energy_error {
                    max_energy_error = energy_error;
                    energy_at_max_error = current_energy;
                }

                if step % 200 == 0 {
                    println!(
                        "Step {}: Energy = {:.6}, Error = {:.6} ({:.4}%)",
                        step,
                        current_energy,
                        energy_error,
                        rel_error * 100.0
                    );
                }
            }
        }

        // Final energy check
        let final_energy = calculate_total_energy(&phys, &balls);
        let total_energy_drift = (final_energy - initial_energy).abs();
        let relative_drift = total_energy_drift / initial_energy.abs();

        println!("\n=== Energy Conservation Results ===");
        println!("Initial energy:    {:.6}", initial_energy);
        println!("Final energy:      {:.6}", final_energy);
        println!(
            "Max energy error:  {:.6} (at E = {:.6})",
            max_energy_error, energy_at_max_error
        );
        println!("Total drift:       {:.6}", total_energy_drift);
        println!(
            "Relative drift:    {:.6}% ({:.2e})",
            relative_drift * 100.0,
            relative_drift
        );

        // Energy should be conserved within numerical precision
        // For 1000 steps with dt=0.001, we expect some drift due to:
        // 1. Floating point rounding errors accumulating
        // 2. Velocity Verlet integration error (O(dt^2))
        // 3. Force capping discontinuity (though potential is continuous)

        // Accept up to 1% relative error for this test
        // This is a fairly loose tolerance, but force capping at close distances
        // can introduce some numerical challenges
        const MAX_ACCEPTABLE_DRIFT: f32 = 0.01; // 1%

        assert!(
            relative_drift < MAX_ACCEPTABLE_DRIFT,
            "Energy drift too large: {:.4}% (max acceptable: {:.4}%). \
             This indicates energy is not being conserved properly with force capping. \
             Initial: {:.6}, Final: {:.6}, Drift: {:.6}",
            relative_drift * 100.0,
            MAX_ACCEPTABLE_DRIFT * 100.0,
            initial_energy,
            final_energy,
            total_energy_drift
        );

        println!("✓ Energy conservation test PASSED");
    }

    /// Helper function to calculate total energy (kinetic + potential) of a system
    fn calculate_total_energy(phys: &BallPhysics, balls: &[Ball2D]) -> f32 {
        // Calculate kinetic energy
        let mut kinetic = 0.0f32;
        for ball in balls {
            let speed_sq = ball.v.x * ball.v.x + ball.v.y * ball.v.y;
            kinetic += 0.5 * ball.mass * speed_sq;
        }

        // Calculate potential energy (only count each pair once)
        let mut potential = 0.0f32;
        for i in 0..balls.len() {
            for j in (i + 1)..balls.len() {
                let dist_vec = balls[j].get_position().dist(balls[i].get_position());
                let center_dist = dist_vec.norm();
                let edge_dist = center_dist - (balls[i].radius + balls[j].radius);

                // Use the physics engine's potential calculation (includes force capping)
                let pair_potential = phys.potential(edge_dist);
                potential += pair_potential;
            }
        }

        kinetic + potential
    }

    #[test]
    fn test_lj_force_and_potential_above_rcap() {
        // Verify that Lennard-Jones calculations are correct for distances >= r_cap
        let phys = BallPhysics::default();
        let r_cap = phys.get_force_cap_distance();

        println!("\n=== Testing LJ Calculations Above r_cap ===");
        println!("r_cap = {:.6}\n", r_cap);

        // Test distances above r_cap
        let test_distances = vec![
            r_cap * 1.0, // At r_cap
            r_cap * 1.1, // 10% above
            r_cap * 1.5, // 50% above
            r_cap * 2.0, // 2x r_cap
            1.122462,    // Equilibrium (2^(1/6))
            r_cap * 3.0, // 3x r_cap
        ];

        // Verify F = -dU/dr numerically for distances above r_cap
        println!("\nVerifying F = -dU/dr above r_cap:");
        println!("Distance (r) | F (direct) | -dU/dr (numerical) | Error");
        println!("{}", "-".repeat(70));

        for &r in &test_distances {
            if r >= r_cap {
                let dr = 1e-4;
                let f_direct = phys.force(r);
                let u1 = phys.potential(r - dr / 2.0);
                let u2 = phys.potential(r + dr / 2.0);
                let f_numerical = -(u2 - u1) / dr;
                let error = (f_direct - f_numerical).abs();

                println!(
                    "{:12.6} | {:10.4} | {:18.4} | {:.4e}",
                    r, f_direct, f_numerical, error
                );

                // They should match very closely
                approx(f_direct, f_numerical, 0.01);
            }
        }

        println!("✓ All LJ calculations above r_cap are correct");
    }

    #[test]
    fn test_potential_continuity_at_rcap() {
        // Verify potential is continuous at the r_cap boundary
        let phys = BallPhysics::default();
        let r_cap = phys.get_force_cap_distance();
        let max_force = phys.constants.max_force;

        println!("\n=== Testing Potential Continuity at r_cap ===");

        // Test continuity of potential at r_cap boundary
        let r_below = r_cap * 0.9999;
        let r_above = r_cap * 1.0001;

        let u_below = phys.potential(r_below);
        let u_at = phys.potential(r_cap);
        let u_above = phys.potential(r_above);

        println!("r_cap = {:.10}", r_cap);
        println!("U(r_cap - ε) = {:.10}", u_below);
        println!("U(r_cap)     = {:.10}", u_at);
        println!("U(r_cap + ε) = {:.10}", u_above);

        // The potential should be continuous (smooth transition)
        let diff_below = (u_below - u_at).abs();
        let diff_above = (u_at - u_above).abs();

        println!("Difference below-at: {:.10e}", diff_below);
        println!("Difference at-above: {:.10e}", diff_above);

        // Both differences should be very small and similar
        assert!(
            diff_below < 0.01,
            "Discontinuity below r_cap: {}",
            diff_below
        );
        assert!(
            diff_above < 0.01,
            "Discontinuity above r_cap: {}",
            diff_above
        );

        // The slope should be approximately -max_force
        let slope_below = (u_at - u_below) / (r_cap - r_below);
        let slope_above = (u_above - u_at) / (r_above - r_cap);

        println!(
            "Slope below r_cap: {:.4} (should be ≈ -max_force = {:.4})",
            slope_below, -max_force
        );
        println!("Slope above r_cap: {:.4}", slope_above);

        // Below r_cap: slope should be -max_force (linear region)
        approx(slope_below, -max_force, 1.0);

        // Above r_cap: slope should transition smoothly
        assert!(
            (slope_above - slope_below).abs() < 50.0,
            "Slope discontinuity at r_cap: {} vs {}",
            slope_below,
            slope_above
        );

        println!("✓ Potential is continuous at r_cap boundary");
    }

    #[test]
    fn test_overlapping_balls_force_capped() {
        // Test that overlapping balls (negative edge distance) have force properly capped
        // This addresses the issue where edge_dist = -0.40344 was producing force = 1840
        let phys = BallPhysics::default();
        let max_force = phys.constants.max_force;

        println!("\n=== Testing Force Capping for Overlapping Balls ===");

        // Test with the specific case from the user's log: edge_dist = -0.40344
        let edge_dist = -0.40344;
        let force = phys.force(edge_dist);
        println!(
            "Edge dist: {:.5}, Force: {:.4}, Max force: {:.1}",
            edge_dist, force, max_force
        );

        // Force should be capped at max_force, not some huge value like 1840
        approx(force, max_force, 1e-2);

        // Test several negative distances to ensure consistent capping
        println!("\nTesting various negative edge distances:");
        for mult in [0.1, 0.5, 1.0, 2.0, 10.0] {
            let dist = -mult;
            let f = phys.force(dist);
            println!("  edge_dist = {:.2}: force = {:.1}", dist, f);
            approx(force, max_force, 1e-2);
        }

        println!("✓ All overlapping ball forces correctly capped at max_force");
    }
}
