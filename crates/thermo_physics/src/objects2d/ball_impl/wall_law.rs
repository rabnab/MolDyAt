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

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::molecule::Site;
use crate::objects2d::ball_impl::pair_potential::{
    CapSettings, CappedPotential, LennardJones, PairLaw, SQRT6_OF_TWO,
};
use crate::objects2d::ball_impl::species::SpeciesId;

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

/// One site as the walls see it: its body-frame offset and its wall
/// potential in the distance `z` from the wall surface (negative behind it:
/// the sloped core keeps pushing it back).
pub(crate) type WallSite = (Vec2D, CappedPotential<LennardJones>);

/// The soft wall potential of every species of a table.
#[derive(Clone, Debug)]
pub(crate) struct SoftWalls {
    /// Per species: its sites (`Species::sites`).
    sites: Vec<Vec<WallSite>>,
    /// Per species: the largest centre distance of a site's potential
    /// minimum (the WCA cutoff).
    equilibrium: Vec<f32>,
}

impl SoftWalls {
    /// `sites` per species; a single site carries the species' like pair
    /// override.
    pub(crate) fn new(sites: &[Vec<Site>], settings: &WallSettings, cap: CapSettings) -> Self {
        let cap = if settings.attractive {
            cap
        } else {
            CapSettings {
                cutoff_factor: SQRT6_OF_TWO,
                ..cap
            }
        };
        let sites: Vec<Vec<WallSite>> = sites
            .iter()
            .enumerate()
            .map(|(i, sites)| {
                let o = settings.overrides.iter().find(|o| o.species.index() == i);
                sites
                    .iter()
                    .map(|site| {
                        let law = LennardJones {
                            sigma: o
                                .and_then(|o| o.sigma)
                                .unwrap_or(0.5 * (site.sigma + settings.sigma)),
                            epsilon: o
                                .and_then(|o| o.epsilon)
                                .unwrap_or((site.epsilon * settings.epsilon).sqrt()),
                        };
                        (site.offset, CappedPotential::new(law, cap))
                    })
                    .collect()
            })
            .collect();
        Self {
            equilibrium: sites
                .iter()
                .map(|s| {
                    s.iter()
                        .map(|(o, p)| o.norm() + p.law().equilibrium_distance())
                        .fold(0.0, f32::max)
                })
                .collect(),
            sites,
        }
    }

    /// The sites of `species`.
    #[inline]
    pub(crate) fn sites(&self, species: SpeciesId) -> &[WallSite] {
        &self.sites[species.index()]
    }

    /// Centre distance from the wall at which every site of `species` is at
    /// or beyond its potential minimum.
    pub(crate) fn equilibrium_distance(&self, species: SpeciesId) -> f32 {
        self.equilibrium[species.index()]
    }
}
