// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Rect;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::pair_potential::{CapSettings, LJ_SIG, Potential};
use crate::objects2d::ball_impl::species::{DEFAULT_SPECIES, SpeciesId, SpeciesSet, SpeciesTable};
use crate::objects2d::ball_impl::wall_law::{HardContact, WallLaw};
use crate::objects2d::ball_impl::{Ball2D, Named};

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
    /// Smallest centre distance to a neighbour, minus that pair's `σ_ij`
    /// (negative inside the repulsive core)
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
    wall_law: HardContact,
    /// Wall contact distance of each species, from `wall_law`.
    wall_contact: Vec<f32>,
}

impl Default for BallPhysics {
    fn default() -> Self {
        BallPhysics::new(
            PhysicsConstants::default(),
            SpeciesTable::builtin(),
            CapSettings::default(),
        )
    }
}

impl BallPhysics {
    /// Lennard-Jones between every pair of `species` (Lorentz–Berthelot
    /// mixing), capped and cut off per `cap`, and hard wall contacts.
    pub fn new(constants: PhysicsConstants, species: &SpeciesTable, cap: CapSettings) -> Self {
        let wall_law = HardContact;
        BallPhysics {
            constants,
            pair_potential: Potential::lennard_jones(species, cap),
            wall_contact: species
                .iter()
                .map(|s| wall_law.contact_distance(s))
                .collect(),
            wall_law,
        }
    }

    /// Force cap and cutoff settings of the pair potential.
    pub fn cap_settings(&self) -> &CapSettings {
        self.pair_potential.cap_settings()
    }

    /// Species `a` and `b` interact only below this centre distance.
    pub fn cutoff(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        self.pair_potential.cutoff(a, b)
    }

    /// Verlet skin: a fixed share of the reference cutoff (`N2`), the same for
    /// every pair, so the rebuild threshold does not depend on the species.
    pub fn get_skin_thickness(&self) -> f32 {
        0.2 * self.cap_settings().cutoff_factor * LJ_SIG
    }

    /// Centre distance below which a pair of `a` and `b` is listed as
    /// neighbours: its cutoff plus the skin.
    pub fn list_radius(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        self.cutoff(a, b) + self.get_skin_thickness()
    }

    /// Largest [`Self::list_radius`] over the pairs of `present`; the cell
    /// grid needs cells at least this wide. For an empty set, that of the
    /// default species.
    pub fn max_list_radius(&self, present: SpeciesSet) -> f32 {
        if present.is_empty() {
            return self.list_radius(DEFAULT_SPECIES, DEFAULT_SPECIES);
        }
        present
            .iter()
            .flat_map(|a| present.iter().map(move |b| (a, b)))
            .map(|(a, b)| self.list_radius(a, b))
            .fold(0.0, f32::max)
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
    /// # Returns
    /// Movement threshold in world units (skin_thickness / 3)
    pub fn get_min_movement_for_update_hood(&self) -> f32 {
        self.get_skin_thickness() / 3.0
    }

    /// Centre distance of the potential minimum of a pair of `a` and `b`.
    pub(crate) fn equilibrium_distance(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        self.pair_potential.equilibrium_distance(a, b)
    }

    /// Pair force magnitude (positive = repulsive) at centre distance `r`; see
    /// [`CappedPotential`](super::pair_potential::CappedPotential).
    #[cfg(test)]
    pub(crate) fn force(&self, a: SpeciesId, b: SpeciesId, r: f32) -> f32 {
        self.pair_potential.force(a, b, r)
    }

    /// Pair potential energy at centre distance `r`, zero at the cutoff.
    #[cfg(test)]
    pub(crate) fn potential(&self, a: SpeciesId, b: SpeciesId, r: f32) -> f32 {
        self.pair_potential.potential(a, b, r)
    }

    /// The pair potentials; the force kernel matches on them once per ball.
    pub(crate) fn pair_potential(&self) -> &Potential {
        &self.pair_potential
    }

    /// Distance below which the force of the pair follows the sloped core.
    #[cfg(test)]
    pub(crate) fn get_force_cap_distance(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        self.pair_potential.r_cap(a, b)
    }

    /// The law that decides how balls meet walls.
    pub fn wall_law(&self) -> &impl WallLaw {
        &self.wall_law
    }

    /// Centre distance from a wall surface at which a ball of `species` is
    /// reflected (see [`WallLaw::contact_distance`]).
    #[inline]
    pub fn wall_contact_distance(&self, species: SpeciesId) -> f32 {
        self.wall_contact[species.index()]
    }

    /// Gap between the contact discs of two balls (radius `σ/2` each): the
    /// centre distance minus `σ_ij`, zero where the pair potential crosses
    /// zero, negative if they overlap.
    pub fn distance_of_edges(a: &Ball2D, b: &Ball2D) -> f32 {
        a.get_edge_distance(b)
    }

    /// Check if two balls are colliding within a margin.
    ///
    /// # Arguments
    /// * `this` - First ball
    /// * `other` - Second ball
    /// * `margin` - Collision margin (positive = earlier detection)
    ///
    /// # Returns
    /// `true` if the gap between their contact discs is less than margin
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
        let contact = self.wall_contact_distance(ball.species());
        let bounce_info = ball.execute_update_pos(dt, bound, contact, restitution);

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
}
