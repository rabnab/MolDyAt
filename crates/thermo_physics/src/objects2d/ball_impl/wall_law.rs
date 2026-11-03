// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! How balls meet walls (box walls, wall pieces, closed valves).
//!
//! Two models ([`WallSettings::soft`]):
//! - **Hard** ([`HardContact`]): a ball is reflected at centre distance
//!   `σ_i / 2` from the wall surface, the closest approach of two like balls
//!   at about T* = 1 (≈ 0.97·σ). No force, no energy.
//! - **Soft** ([`SoftWalls`], the default for new worlds, #280): the wall is
//!   a pseudo-species with `σ_w`, `ε_w`, and a ball feels the Lennard-Jones
//!   law of the mixed `σ_iw = (σ_i + σ_w)/2`, `ε_iw = √(ε_i·ε_w)` in its
//!   distance `z` from the wall surface, capped like a pair
//!   (`CappedPotential`). By default it is cut and shifted at its minimum
//!   (WCA, purely repulsive); `attractive` keeps the well up to the pair
//!   cutoff. The hard contact stays as a backstop at `backstop · σ_i`, where
//!   the wall energy is far above `k_B·T`: a soft wall alone has a finite
//!   barrier (the force cap), is sampled once per step, and flips its force
//!   once a centre crosses a thin segment.
//!
//! The wall code asks [`WallLaw::contact_distance`] where the hard contact
//! is; the soft force is added in the force pass (`wall_forces`).

use crate::objects2d::ball_impl::pair_potential::{
    CapSettings, CappedPotential, LennardJones, PairPotential, SQRT6_OF_TWO,
};
use crate::objects2d::ball_impl::species::{Species, SpeciesId, SpeciesTable};

/// The interaction of balls with walls.
pub(crate) trait WallLaw {
    /// Centre distance from a wall surface at which a ball of `species`
    /// touches the wall and is reflected.
    fn contact_distance(&self, species: &Species) -> f32;
}

/// Elastic (or damped, by the restitution) reflection at `σ_i / 2`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HardContact;

impl WallLaw for HardContact {
    fn contact_distance(&self, species: &Species) -> f32 {
        species.contact_radius()
    }
}

/// The hard backstop behind a soft wall: reflection at `factor · σ_i`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Backstop {
    pub factor: f32,
}

impl WallLaw for Backstop {
    fn contact_distance(&self, species: &Species) -> f32 {
        self.factor * species.sigma
    }
}

/// Default `σ_w`: no size of its own, so `σ_iw = σ_i / 2`, the hard contact
/// distance; the soft wall then has the hard wall's contact distance.
pub(crate) const DEFAULT_WALL_SIGMA: f32 = 0.0;
/// Default `ε_w`, the well depth of the reference species.
pub(crate) const DEFAULT_WALL_EPSILON: f32 = 1.0;
/// Default backstop distance as a share of `σ_i`: with the default wall and
/// cap the wall energy there is ≈ 24 ε for `N2`, so thermal balls reach it
/// with a probability of about e⁻²⁴ per contact at T* = 1.
pub(crate) const DEFAULT_BACKSTOP_FACTOR: f32 = 0.3;

/// The wall model of a world, fixed when its physics is built.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WallSettings {
    /// Soft wall potential (with the hard backstop) or the hard contact only.
    pub soft: bool,
    /// `σ_w` of the wall pseudo-species.
    pub sigma: f32,
    /// `ε_w` of the wall pseudo-species.
    pub epsilon: f32,
    /// Keep the attractive well (cut at the pair cutoff) instead of WCA.
    pub attractive: bool,
    /// Backstop distance as a share of `σ_i`.
    pub backstop: f32,
    /// Species whose `σ_iw` / `ε_iw` do not follow the mixing rule.
    pub overrides: Vec<WallOverride>,
}

impl WallSettings {
    /// The hard contact only, as every world had before #280.
    pub(crate) fn hard() -> Self {
        Self {
            soft: false,
            ..Self::default()
        }
    }
}

impl Default for WallSettings {
    /// Soft WCA walls with `σ_w = 0`, `ε_w = 1`.
    fn default() -> Self {
        Self {
            soft: true,
            sigma: DEFAULT_WALL_SIGMA,
            epsilon: DEFAULT_WALL_EPSILON,
            attractive: false,
            backstop: DEFAULT_BACKSTOP_FACTOR,
            overrides: Vec::new(),
        }
    }
}

/// Replaces the mixed `σ_iw` and / or `ε_iw` of one species.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WallOverride {
    pub species: SpeciesId,
    pub sigma: Option<f32>,
    pub epsilon: Option<f32>,
}

/// The soft wall potential of every species of a table.
#[derive(Clone, Debug)]
pub(crate) struct SoftWalls {
    /// Wall–species potential in the distance `z` from the wall surface.
    potentials: Vec<CappedPotential<LennardJones>>,
    /// Distance of the potential minimum (the WCA cutoff).
    equilibrium: Vec<f32>,
}

impl SoftWalls {
    /// `sigma_of(id)` is the species' effective `σ_i` (a `σ_ii` pair
    /// override replaces the table's).
    pub(crate) fn new(
        species: &SpeciesTable,
        sigma_of: impl Fn(SpeciesId) -> f32,
        settings: &WallSettings,
        cap: CapSettings,
    ) -> Self {
        let cap = if settings.attractive {
            cap
        } else {
            CapSettings {
                cutoff_factor: SQRT6_OF_TWO,
                ..cap
            }
        };
        let laws: Vec<LennardJones> = species
            .ids()
            .map(|id| {
                let s = species.species(id);
                let mixed = LennardJones {
                    sigma: 0.5 * (sigma_of(id) + settings.sigma),
                    epsilon: (s.epsilon * settings.epsilon).sqrt(),
                };
                match settings.overrides.iter().find(|o| o.species == id) {
                    Some(o) => LennardJones {
                        sigma: o.sigma.unwrap_or(mixed.sigma),
                        epsilon: o.epsilon.unwrap_or(mixed.epsilon),
                    },
                    None => mixed,
                }
            })
            .collect();
        Self {
            equilibrium: laws.iter().map(|l| SQRT6_OF_TWO * l.sigma).collect(),
            potentials: laws
                .into_iter()
                .map(|l| CappedPotential::new(l, cap))
                .collect(),
        }
    }

    /// Wall distance at and beyond which a ball of `species` feels nothing.
    #[inline]
    pub(crate) fn cutoff(&self, species: SpeciesId) -> f32 {
        self.potentials[species.index()].cutoff()
    }

    /// Wall distance of the potential minimum of `species`.
    pub(crate) fn equilibrium_distance(&self, species: SpeciesId) -> f32 {
        self.equilibrium[species.index()]
    }

    /// Force (positive = away from the wall) and potential energy of a ball
    /// of `species` at signed distance `z` from the wall surface (negative
    /// behind it: the sloped core keeps pushing it back).
    #[inline]
    pub(crate) fn force_and_potential(&self, species: SpeciesId, z: f32) -> (f32, f32) {
        let p = &self.potentials[species.index()];
        (p.force(z), p.potential(z))
    }
}
