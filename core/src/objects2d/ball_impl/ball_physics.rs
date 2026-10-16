// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Rect;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::pair_potential::{
    CapSettings, LennardJones, PairPotential, Potential,
};
use crate::objects2d::ball_impl::{Ball2D, Named};
use std::simd::f32x4;

/// Physics settings that do not belong to the pair potential; they may change
/// while the simulation runs. The force cap and cutoff live in the potential
/// ([`CapSettings`]).
#[derive(Clone, Default)]
pub struct PhysicsConstants {
    pub gravity: f32,
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
    pair_potential: Potential,
}

impl Default for BallPhysics {
    fn default() -> Self {
        BallPhysics::new(
            PhysicsConstants::default(),
            LennardJones::default(),
            CapSettings::default(),
        )
    }
}

impl BallPhysics {
    pub fn new(constants: PhysicsConstants, lj: LennardJones, cap: CapSettings) -> Self {
        let pair_potential = Potential::lennard_jones(lj, cap);
        BallPhysics {
            constants,
            pair_potential,
        }
    }

    /// Force cap and cutoff settings of the pair potential.
    pub fn cap_settings(&self) -> &CapSettings {
        self.pair_potential.cap_settings()
    }

    /// Pairs interact only below this edge distance.
    pub fn get_cutoff_distance(&self) -> f32 {
        self.pair_potential.cutoff()
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
        self.pair_potential.equilibrium_distance()
    }

    /// Pair force magnitude (positive = repulsive) at `edge_dist`; see
    /// [`CappedPotential`](super::pair_potential::CappedPotential).
    pub(crate) fn force(&self, edge_dist: f32) -> f32 {
        self.pair_potential.force(edge_dist)
    }

    /// SIMD version of [`Self::force`] for four edge distances.
    pub(crate) fn force_simd(&self, edge_dist: f32x4) -> f32x4 {
        self.pair_potential.force_simd(edge_dist)
    }

    /// Pair potential energy at `edge_dist`, zero at the cutoff.
    pub(crate) fn potential(&self, edge_dist: f32) -> f32 {
        self.pair_potential.potential(edge_dist)
    }

    /// SIMD version of [`Self::potential`] for four edge distances.
    pub(crate) fn potential_simd(&self, edge_dist: f32x4) -> f32x4 {
        self.pair_potential.potential_simd(edge_dist)
    }

    /// Distance below which the force follows the sloped core.
    #[cfg(test)]
    pub(crate) fn get_force_cap_distance(&self) -> f32 {
        self.pair_potential.r_cap()
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
