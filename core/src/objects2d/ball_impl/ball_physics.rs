// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Rect;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::{Ball2D, Named};
use std::simd::Select;
use std::simd::f32x4;
use std::simd::prelude::{SimdFloat, SimdPartialOrd};

//const LJ_SIG: f32 = 5.; // equilibrium distance
//LJ_SIG in SI units is for Argon 3.4A=3.4E-10m
pub(crate) const LJ_SIG: f32 = 1.; // equilibrium distance
const LJ_EPS: f32 = 1.; // attractive strength
const MAX_FORCE: f32 = 100.0; // max repulsive force to cap at
/// Slope of the repulsion below `r_cap`: the force grows by this much per unit
/// of extra compression, so deeper overlaps push harder than shallow ones.
/// Stability: smallest reduced mass 0.5 gives ω·dt ≈ 0.2 at dt = 0.01.
const CORE_STIFFNESS: f32 = 200.0;
const SQRT6_OF_TWO: f32 = 1.122_462; // == 2^(1/6)

#[derive(Clone)]
pub struct PhysicsConstants {
    pub lj_sig: f32,
    pub lj_eps: f32,
    pub lj_cutoff: f32,
    pub gravity: f32,
    pub max_force: f32,
    /// Force slope below `r_cap` (see [`BallPhysics::force`]).
    pub core_stiffness: f32,
    pub pot_at_cap: f32,
    /// Cached distance at which LJ force equals max_force
    pub r_cap: f32,
    /// Cached U_LJ(lj_cutoff): subtracted from every pair potential inside the
    /// cutoff so the truncated potential goes to zero at `lj_cutoff`.
    pub pot_at_cutoff: f32,
}

impl PhysicsConstants {
    #[cfg(test)]
    pub(crate) fn new_for_test(lj_sig: f32, lj_eps: f32, max_force: f32) -> Self {
        let r_cap = Self::calculate_force_cap_distance(lj_sig, lj_eps, max_force);
        let pot_at_cap = {
            let cap_quot = lj_sig / r_cap;
            let cap_quot_six = cap_quot * cap_quot * cap_quot * cap_quot * cap_quot * cap_quot;
            4. * lj_eps * cap_quot_six * (cap_quot_six - 1.)
        };

        let lj_cutoff = 4. * lj_sig;
        PhysicsConstants {
            lj_eps,
            lj_sig,
            gravity: 0.,
            max_force,
            core_stiffness: CORE_STIFFNESS,
            lj_cutoff,
            pot_at_cap,
            r_cap,
            pot_at_cutoff: Self::lj_potential(lj_sig, lj_eps, lj_cutoff),
        }
    }

    /// Plain (uncapped, unshifted) Lennard-Jones potential 4·ε·[(σ/r)^12 - (σ/r)^6].
    fn lj_potential(lj_sig: f32, lj_eps: f32, r: f32) -> f32 {
        let quot_six = (lj_sig / r).powi(6);
        4. * lj_eps * quot_six * (quot_six - 1.)
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
    /// For r < r_cap, the force continues from max_force with a constant slope k
    /// (`core_stiffness`), with the matching quadratic potential:
    ///   U(r) = U(r_cap) - max_force·(r - r_cap) + ½·k·(r - r_cap)²
    /// This ensures F = -dU/dr = max_force + k·(r_cap - r) (repulsive), energy
    /// continuity at r_cap, and conservative forces.
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

        let lj_cutoff = 4. * LJ_SIG; // Standard cutoff for L-J potential 2.5
        PhysicsConstants {
            lj_eps: LJ_EPS,
            lj_sig: LJ_SIG,
            //gravity: 9.81,
            gravity: 0.0,
            //gravity: 1.0,
            max_force: MAX_FORCE,
            core_stiffness: CORE_STIFFNESS,
            //softening: 0.01,
            lj_cutoff,
            pot_at_cap,
            r_cap,
            pot_at_cutoff: Self::lj_potential(LJ_SIG, LJ_EPS, lj_cutoff),
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
    /// Sum of virial contributions from all neighbours: Σ_j (r_ij · F_ij).
    /// Here `r_ij` is the displacement from neighbour `j` to ball `i` (`r_i - r_j`),
    /// matching the force direction convention used in this module. The 0.5
    /// double-counting factor is NOT applied here; it must be applied when
    /// accumulating into per-cell statistics. The 2D virial theorem's own 1/2
    /// (1/d) is applied in `thermo_field::virial_pressure_2d`.
    pub virial: f32,
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
        0.2 * self.get_cutoff_distance()
    }

    pub fn get_cutoff_and_skin(&self) -> f32 {
        self.get_cutoff_distance() + self.get_skin_thickness()
    }

    /// Returns the minimum movement threshold before a ball's neighbor list needs updating.
    ///
    /// Balls rebuild their lists independently, not all at once. When ball j
    /// rebuilds (pair at least cutoff + skin apart), ball i may already be up
    /// to 2 thresholds away from where it was at its own last rebuild, and j
    /// may then move one more threshold. Keeping 3 thresholds within the skin
    /// guarantees no pair gets inside the cutoff without being listed; half
    /// the skin (the textbook value for synchronous rebuilds) does not.
    ///
    /// # Arguments
    /// * `_max_radius` - Maximum ball radius (currently unused, reserved for dynamic calculations)
    ///
    /// # Returns
    /// Movement threshold in world units (skin_thickness / 3)
    pub fn get_min_movement_for_update_hood(&self, _max_radius: f32) -> f32 {
        self.get_skin_thickness() / 3.0
    }

    pub(crate) fn equilibrium_edge_distance(&self) -> f32 {
        //case: r1==r2 --- SQRT6_OF_TWO * LJ_SIG + 2* rad == dist_equi
        SQRT6_OF_TWO * self.constants.lj_sig
    }

    /// Pair force magnitude (positive = repulsive) at `edge_dist`: Lennard-Jones
    /// down to `r_cap`, below it `max_force + core_stiffness·(r_cap - edge_dist)`,
    /// and zero at and beyond `lj_cutoff`.
    ///
    /// The slope below `r_cap` matters in crowded regions: with a flat cap every
    /// close contact pushes equally hard however deep it is, so a deeply
    /// overlapping pair squeezed by its neighbours feels no net force to
    /// separate (issue #240).
    pub(crate) fn force(&self, edge_dist: f32) -> f32 {
        if edge_dist >= self.constants.lj_cutoff {
            return 0.;
        }
        let r_cap = self.get_force_cap_distance();
        let lj_sig = self.constants.lj_sig;
        let lj_eps = self.constants.lj_eps;
        let edge_dist_cap = edge_dist.max(r_cap);

        let quot = lj_sig / edge_dist_cap;
        let quot_six = quot * quot * quot * quot * quot * quot; // (σ/r)^6
        let lj_force = 24. * lj_eps * quot_six * (2. * quot_six - 1.) / edge_dist_cap;
        lj_force + self.constants.core_stiffness * (r_cap - edge_dist).max(0.)
    }

    /// SIMD version of [`Self::force`] for four raw edge distances.
    pub(crate) fn force_simd(&self, edge_dist: f32x4) -> f32x4 {
        let lj_sig = self.constants.lj_sig;
        let lj_eps = self.constants.lj_eps;
        let edge_dist_cap = edge_dist.simd_max(f32x4::splat(self.get_force_cap_distance()));
        let quot = f32x4::splat(lj_sig) / edge_dist_cap;
        let quot_six = quot * quot * quot * quot * quot * quot; // (σ/r)^6
        let lj_force = f32x4::splat(24. * lj_eps)
            * quot_six
            * (f32x4::splat(2.) * quot_six - f32x4::splat(1.))
            / edge_dist_cap;
        let compression = (edge_dist_cap - edge_dist).simd_max(f32x4::splat(0.));
        let force = lj_force + f32x4::splat(self.constants.core_stiffness) * compression;
        self.inside_cutoff(edge_dist)
            .select(force, f32x4::splat(0.))
    }

    fn inside_cutoff(&self, edge_dist: f32x4) -> std::simd::Mask<i32, 4> {
        edge_dist.simd_lt(f32x4::splat(self.constants.lj_cutoff))
    }

    /// Calculate Lennard-Jones potential energy with force capping.
    ///
    /// The standard Lennard-Jones potential is:
    ///   U(r) = 4·ε·[(σ/r)^12 - (σ/r)^6]
    ///
    /// At equilibrium (r = 2^(1/6)·σ), U = -ε (minimum potential energy).
    ///
    /// ### Force Capping for Energy Conservation
    /// When r < r_cap (where force would exceed max_force), we use a quadratic potential:
    ///   U(r) = U(r_cap) - max_force·(r - r_cap) + ½·k·(r - r_cap)²
    /// with k = `core_stiffness`.
    ///
    /// This extrapolation ensures:
    /// 1. Energy continuity at r = r_cap: U(r_cap⁻) = U(r_cap⁺)
    /// 2. Force continuity: F = -dU/dr = max_force + k·(r_cap - r) (repulsive) for r < r_cap
    /// 3. Conservative force field (energy is conserved)
    ///
    /// ### Cutoff
    /// Pairs interact only below `lj_cutoff`. The potential is shifted by
    /// U(lj_cutoff) so it reaches zero there: a pair entering or leaving a
    /// neighbour list (which only happens beyond the cutoff) no longer changes
    /// the total energy.
    ///
    /// # Arguments
    /// * `edge_dist` - Edge-to-edge distance between ball surfaces
    pub(crate) fn potential(&self, edge_dist: f32) -> f32 {
        if edge_dist >= self.constants.lj_cutoff {
            return 0.;
        }
        let r_cap = self.constants.r_cap;
        let potential = if edge_dist < r_cap {
            // U(r) = U(r_cap) - max_force·(r - r_cap) + ½·k·(r - r_cap)²,
            // so F = -dU/dr = max_force + k·(r_cap - r) below r_cap
            let compression = r_cap - edge_dist;
            self.constants.pot_at_cap
                + compression
                    * (self.constants.max_force + 0.5 * self.constants.core_stiffness * compression)
        } else {
            let pot_quot = self.constants.lj_sig / edge_dist;
            let pot_quot_six = pot_quot * pot_quot * pot_quot * pot_quot * pot_quot * pot_quot;
            4. * self.constants.lj_eps * pot_quot_six * (pot_quot_six - 1.)
        };
        potential - self.constants.pot_at_cutoff
    }

    pub(crate) fn potential_simd(&self, edge_dist: f32x4) -> f32x4 {
        // === Potential Energy Calculation with Force Capping ===
        // Standard LJ potential: U(r) = 4·ε·[(σ/r)^12 - (σ/r)^6]
        // For r < r_cap: quadratic continuation matching the sloped core force
        let r_cap_vec = f32x4::splat(self.get_force_cap_distance());
        let lj_sig = self.constants.lj_sig;
        let lj_eps = self.constants.lj_eps;
        let max_force_vec = f32x4::splat(self.constants.max_force);
        let half_stiffness_vec = f32x4::splat(0.5 * self.constants.core_stiffness);
        let pot_at_cap_vec = f32x4::splat(self.constants.pot_at_cap);

        let in_core = edge_dist.simd_lt(r_cap_vec);
        // LJ evaluated at max(r, r_cap) only, so it stays finite in the core
        let pot_quot = f32x4::splat(lj_sig) / edge_dist.simd_max(r_cap_vec);
        let pot_quot_six = pot_quot * pot_quot * pot_quot * pot_quot * pot_quot * pot_quot;
        let potential_lj =
            f32x4::splat(4. * lj_eps) * pot_quot_six * (pot_quot_six - f32x4::splat(1.));
        // Below r_cap: U(r) = U(r_cap) - max_force·(r - r_cap) + ½·k·(r - r_cap)²,
        // so F = -dU/dr = max_force + k·(r_cap - r)
        let compression = r_cap_vec - edge_dist;
        let potential_core =
            pot_at_cap_vec + compression * (max_force_vec + half_stiffness_vec * compression);

        let potential_shifted = in_core.select(potential_core, potential_lj)
            - f32x4::splat(self.constants.pot_at_cutoff);
        self.inside_cutoff(edge_dist)
            .select(potential_shifted, f32x4::splat(0.))
    }

    /// Get the cached force cap distance
    pub(crate) fn get_force_cap_distance(&self) -> f32 {
        self.constants.r_cap
    }

    /// Calculate edge-to-edge distance between two balls.
    ///
    /// # Arguments
    /// * `a` - First ball
    /// * `b` - Second ball
    ///
    /// # Returns
    /// Distance between ball surfaces (negative if overlapping)
    pub fn distance_of_edges(a: &Ball2D, b: &Ball2D) -> f32 {
        let center_dist = a.get_position().dist(b.get_position()).norm();
        center_dist - (a.radius + b.radius)
    }

    /// Check if two balls are colliding within a margin.
    ///
    /// # Arguments
    /// * `this` - First ball
    /// * `other` - Second ball
    /// * `margin` - Collision margin (positive = earlier detection)
    ///
    /// # Returns
    /// `true` if edge distance is less than margin
    pub fn is_colliding(&self, this: &Ball2D, other: &Ball2D, margin: f32) -> bool {
        this.get_edge_distance(other) < margin
    }

    /// Update ball position using velocity-Verlet integration and handle wall bounces.
    ///
    /// # Arguments
    /// * `ball` - Ball to update
    /// * `dt` - Time step
    /// * `bound` - Boundary rectangle (reflective walls)
    /// * `restitution` - Normal restitution of the walls (1.0 = elastic)
    ///
    /// # Returns
    /// Information about wall bounces that occurred
    pub fn update_positions(
        &self,
        ball: &mut Ball2D,
        dt: f32,
        bound: &Rect,
        restitution: f32,
    ) -> BounceInfo {
        let bounce_info = ball.execute_update_pos(dt, bound, restitution);

        if ball.verbose.console {
            log::debug!(
                target: crate::logging::VERBOSE_TARGET,
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
        let ball_pos = *ball.get_position();
        let ball_radius = ball.radius;
        ball.hood
            .get_neighbour_indices()
            .iter()
            .filter_map(|&idx| balls.get(idx))
            .map(|other| {
                let center_dist = ball_pos.dist(other.get_position()).norm();
                let edge_dist = center_dist - (ball_radius + other.radius);
                (*other.get_position(), self.force(edge_dist))
            })
            .collect()
    }
}
