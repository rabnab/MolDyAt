// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! How balls meet walls (box walls, wall pieces, closed valves).
//!
//! Two models ([`WallSettings::soft`]):
//! - **Soft** ([`SoftWalls`], the default for new worlds): the wall is a
//!   pseudo-species with `σ_w`, `ε_w`, and a ball feels the Lennard-Jones
//!   law of the mixed `σ_iw = (σ_i + σ_w)/2`, `ε_iw = √(ε_i·ε_w)` in its
//!   distance `z` from the wall surface, capped like a pair
//!   (`CappedPotential`). By default it is cut and shifted at its minimum
//!   (WCA, purely repulsive); `attractive` keeps the well up to the pair
//!   cutoff. Walls act only through this force (`wall_forces`). Behind it
//!   sits a hard backstop at `backstop · σ_i`, where the wall energy is far
//!   above `k_B·T`: a counted safety net, since a soft wall alone has a
//!   finite barrier (the force cap), is sampled once per step, and flips its
//!   force once a centre crosses a thin segment.
//! - **Hard** (saves from before soft walls): a ball is reflected at centre
//!   distance `σ_i / 2` from the wall surface, the closest approach of two
//!   like balls at about T* = 1 (≈ 0.97·σ), in the position update. No
//!   force, no energy.
//!
//! The wall code asks [`WallSettings::contact_factor`] where the hard
//! contact (or the backstop) is.

use crate::objects2d::ball_impl::pair_potential::{
    CapSettings, CappedPotential, LennardJones, PairPotential, SQRT6_OF_TWO,
};
use crate::objects2d::ball_impl::species::{SpeciesId, SpeciesTable};

/// Default `σ_w`: no size of its own, so `σ_iw = σ_i / 2`, the hard contact
/// distance; the soft wall then has the hard wall's contact distance.
pub(crate) const DEFAULT_WALL_SIGMA: f32 = 0.0;
/// Default `ε_w`: the well depth of the unit species, Ar (#405). Saves from
/// before format v6 ran with `1 ε_N2` and are migrated to keep it.
pub(crate) const DEFAULT_WALL_EPSILON: f32 = 1.0;
/// Default backstop distance as a share of `σ_i`: with the default wall and
/// cap the wall energy there is ≈ 24 ε for `N2`, so thermal balls reach it
/// with a probability of about e⁻²⁴ per contact at T* = 1.
pub(crate) const DEFAULT_BACKSTOP_FACTOR: f32 = 0.3;

/// Time over which the line walls ramp in for a ball a wall appeared on:
/// long against a step, short enough that a thermal ball is out after
/// about one `σ`.
pub(crate) const WALL_RAMP_TIME: f32 = 1.0;

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

    /// Wall contact distance as a share of `σ` (of a ball or a site):
    /// `σ / 2` with hard walls, the backstop behind soft walls.
    pub(crate) fn contact_factor(&self) -> f32 {
        if self.soft { self.backstop } else { 0.5 }
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

/// One site of a molecule as the walls see it: its offset along the axis
/// and its wall potential (#406).
pub(crate) type WallSite = (f32, CappedPotential<LennardJones>);

/// The soft wall potential of every species of a table.
#[derive(Clone, Debug)]
pub(crate) struct SoftWalls {
    /// Wall–species potential in the distance `z` from the wall surface.
    potentials: Vec<CappedPotential<LennardJones>>,
    /// Distance of the potential minimum (the WCA cutoff).
    equilibrium: Vec<f32>,
    /// Per species, for a molecule (#406): (offset along the axis, wall
    /// potential) of each site. The wall acts on the sites, not the centre.
    sites: Vec<Option<Vec<WallSite>>>,
}

impl SoftWalls {
    /// `like_pair(id)` is the species' effective `(σ_i, ε_i)`: a `σ_ii` or
    /// `ε_ii` pair override replaces the table's value.
    pub(crate) fn new(
        species: &SpeciesTable,
        like_pair: impl Fn(SpeciesId) -> (f32, f32),
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
        let wall_law = |sigma: f32, epsilon: f32| LennardJones {
            sigma: 0.5 * (sigma + settings.sigma),
            epsilon: (epsilon * settings.epsilon).sqrt(),
        };
        let sites = species
            .iter()
            .map(|s| {
                s.model.molecule().map(|m| {
                    m.sites
                        .iter()
                        .map(|site| {
                            (
                                site.offset,
                                CappedPotential::new(wall_law(site.sigma, site.epsilon), cap),
                            )
                        })
                        .collect()
                })
            })
            .collect();
        let laws: Vec<LennardJones> = species
            .ids()
            .map(|id| {
                let (sigma, epsilon) = like_pair(id);
                let mixed = LennardJones {
                    sigma: 0.5 * (sigma + settings.sigma),
                    epsilon: (epsilon * settings.epsilon).sqrt(),
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
            sites,
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

    /// The sites of a molecule species (offset, wall potential), `None` for
    /// an atomic species.
    pub(crate) fn sites(&self, species: SpeciesId) -> Option<&[WallSite]> {
        self.sites[species.index()].as_deref()
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
