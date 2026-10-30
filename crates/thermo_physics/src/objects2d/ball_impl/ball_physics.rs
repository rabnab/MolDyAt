// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Rect;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::pair_potential::{CapSettings, LJ_SIG, PairOverride, Potential};
use crate::objects2d::ball_impl::species::{
    DEFAULT_SPECIES, Species, SpeciesId, SpeciesSet, SpeciesTable,
};
use crate::objects2d::ball_impl::wall_law::{HardContact, WallLaw};
use crate::objects2d::ball_impl::{Ball2D, Named};
use std::sync::Arc;

/// Physics settings that do not belong to the pair potential; they may change
/// while the simulation runs. The force cap and cutoff live in the potential
/// (`CapSettings`).
#[derive(Clone, Default)]
pub struct PhysicsConstants {
    pub gravity: f32,
}

/// Result of force calculation for a ball, including minimum distance to neighbors,
/// total force vector, and total potential energy from interactions.
#[derive(Clone, Debug)]
pub struct ForceResult {
    /// Smallest centre distance to a neighbour, minus the two contact radii
    /// `σ_i/2 + σ_j/2` (negative inside the repulsive core). For a pair with
    /// a `σ_ij` override this is not its `BallPhysics::edge_gap`; only
    /// tests read it.
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

/// Default Verlet skin as a share of the reference cutoff (see
/// [`BallPhysics::get_skin_thickness`]).
pub(crate) const DEFAULT_SKIN_FACTOR: f32 = 0.2;

#[derive(Clone)]
pub struct BallPhysics {
    pub constants: PhysicsConstants,
    /// The species table every per-species array below is built from. The
    /// one owner of the world's table: the world, the ball factory and the
    /// snapshots read it from here, so physics and table cannot diverge.
    species: Arc<SpeciesTable>,
    pair_potential: Potential,
    /// The pairs of `pair_potential` that do not follow the mixing rule.
    pair_overrides: Vec<PairOverride>,
    /// Skin as a share of the reference cutoff.
    skin_factor: f32,
    /// Wall contact distance of each species, from `wall_law`.
    wall_contact: Vec<f32>,
    /// Contact distance (`σ_ij`) of each species pair, row-major.
    contact: Vec<f32>,
    species_count: usize,
}

/// The built-in species with the default caps. Test builds only: a world's
/// physics is built from its config's table.
#[cfg(test)]
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
    #[cfg(test)]
    pub(crate) fn new(
        constants: PhysicsConstants,
        species: &SpeciesTable,
        cap: CapSettings,
    ) -> Self {
        Self::with_pair_overrides(constants, Arc::new(species.clone()), cap, Vec::new())
    }

    /// [`Self::new`], with the pairs in `overrides` taking their `σ_ij` /
    /// `ε_ij` from there instead of the mixing rule. The contact distance of
    /// an overridden pair is its `σ_ij`, and a like pair's `σ_ii` also sets
    /// the species' wall contact.
    pub(crate) fn with_pair_overrides(
        constants: PhysicsConstants,
        species_table: Arc<SpeciesTable>,
        cap: CapSettings,
        overrides: Vec<PairOverride>,
    ) -> Self {
        let species = &*species_table;
        let wall_law = HardContact;
        let sigma_override = |a: SpeciesId, b: SpeciesId| {
            overrides
                .iter()
                .find(|o| o.is_for(a, b))
                .and_then(|o| o.sigma)
        };
        let wall_contact = species
            .ids()
            .map(|id| {
                let s = species.species(id);
                match sigma_override(id, id) {
                    Some(sigma) => wall_law.contact_distance(&Species { sigma, ..s.clone() }),
                    None => wall_law.contact_distance(s),
                }
            })
            .collect();
        // Without an override, the sum of the contact radii, as
        // `Ball2D::get_edge_distance` computes it (`σ_ij` up to rounding).
        let contact = species
            .ids()
            .flat_map(|a| species.ids().map(move |b| (a, b)))
            .map(|(a, b)| {
                sigma_override(a, b).unwrap_or_else(|| {
                    species.species(a).contact_radius() + species.species(b).contact_radius()
                })
            })
            .collect();
        BallPhysics {
            constants,
            species: Arc::clone(&species_table),
            pair_potential: Potential::lennard_jones(species, cap, &overrides),
            pair_overrides: overrides,
            skin_factor: DEFAULT_SKIN_FACTOR,
            wall_contact,
            contact,
            species_count: species.len(),
        }
    }

    /// The same physics with a Verlet skin of `skin_factor` times the
    /// reference cutoff.
    pub(crate) fn with_skin_factor(mut self, skin_factor: f32) -> Self {
        self.skin_factor = skin_factor;
        self
    }

    /// The species table the physics was built from (the world's).
    pub(crate) fn species(&self) -> &Arc<SpeciesTable> {
        &self.species
    }

    pub(crate) fn skin_factor(&self) -> f32 {
        self.skin_factor
    }

    /// Force cap and cutoff settings of the pair potential.
    pub(crate) fn cap_settings(&self) -> &CapSettings {
        self.pair_potential.cap_settings()
    }

    /// The species pairs whose `σ_ij` / `ε_ij` do not come from the mixing
    /// rule.
    pub(crate) fn pair_overrides(&self) -> &[PairOverride] {
        &self.pair_overrides
    }

    /// Species `a` and `b` interact only below this centre distance.
    pub fn cutoff(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        self.pair_potential.cutoff(a, b)
    }

    /// Verlet skin: a fixed share (`skin_factor`) of the reference cutoff
    /// (`N2`), the same for every pair, so the rebuild threshold does not
    /// depend on the species.
    pub(crate) fn get_skin_thickness(&self) -> f32 {
        self.skin_factor * self.cap_settings().cutoff_factor * LJ_SIG
    }

    /// Centre distance below which a pair of `a` and `b` is listed as
    /// neighbours: its cutoff plus the skin.
    pub(crate) fn list_radius(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        self.cutoff(a, b) + self.get_skin_thickness()
    }

    /// Largest [`Self::list_radius`] over the pairs of `present`; the cell
    /// grid needs cells at least this wide. For an empty set, that of the
    /// default species.
    pub(crate) fn max_list_radius(&self, present: SpeciesSet) -> f32 {
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
    pub(crate) fn get_min_movement_for_update_hood(&self) -> f32 {
        self.get_skin_thickness() / 3.0
    }

    /// Centre distance of the potential minimum of a pair of `a` and `b`.
    pub fn equilibrium_distance(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        self.pair_potential.equilibrium_distance(a, b)
    }

    /// The pair potentials; the force kernel matches on them once per ball.
    pub(crate) fn pair_potential(&self) -> &Potential {
        &self.pair_potential
    }

    /// Centre distance from a wall surface at which a ball of `species` is
    /// reflected (see [`WallLaw::contact_distance`]).
    #[inline]
    pub(crate) fn wall_contact_distance(&self, species: SpeciesId) -> f32 {
        self.wall_contact[species.index()]
    }

    /// Centre distance at which the potential of species `a` and `b`
    /// crosses zero (`σ_ij`, also for an overridden pair): where placement
    /// and the overlap checks put contact.
    #[inline]
    pub(crate) fn contact_distance(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        self.contact[a.index() * self.species_count + b.index()]
    }

    /// Gap between two balls: their centre distance minus their
    /// [`Self::contact_distance`]; negative if they overlap.
    pub(crate) fn edge_gap(&self, a: &Ball2D, b: &Ball2D) -> f32 {
        let center_dist = a.get_position().dist(b.get_position()).norm();
        center_dist - self.contact_distance(a.species(), b.species())
    }

    /// Check if two balls are colliding within a margin.
    ///
    /// # Arguments
    /// * `this` - First ball
    /// * `other` - Second ball
    /// * `margin` - Collision margin (positive = earlier detection)
    ///
    /// # Returns
    /// `true` if their [`Self::edge_gap`] is less than margin
    pub(crate) fn is_colliding(&self, this: &Ball2D, other: &Ball2D, margin: f32) -> bool {
        self.edge_gap(this, other) < margin
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
    pub(crate) fn update_positions(
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
                ball.cold.hood.nearest,
                ball.cold.hood.farthest
            );
        }

        bounce_info
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

    /// Distance below which the force of the pair follows the sloped core.
    #[cfg(test)]
    pub(crate) fn get_force_cap_distance(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        self.pair_potential.r_cap(a, b)
    }
}
