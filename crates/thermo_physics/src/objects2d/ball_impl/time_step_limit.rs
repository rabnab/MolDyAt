// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Time step limits derived from the species (#427).
//!
//! Velocity Verlet needs several steps per period of the fastest motion: it
//! goes unstable at `ω·dt = 2` and is accurate only well below. When the
//! physics is built, every pair of species gets the angular frequency `ω`
//! of its stiffest interaction and the limit `dt_max = STEP_MARGIN / ω`
//! (at least [`STEPS_PER_PERIOD`] steps per period). The steps run with at
//! most the smallest limit of the pairs present (`MainContent::active_time_step`).
//!
//! `ω² = k / μ` with the curvature `k = U''` of the pair's capped potential,
//! the larger of
//! - its well minimum (`72·2^(-1/3)·ε/σ²` for Lennard-Jones): bound pairs
//!   oscillate there;
//! - the stiffest point of a head-on collision at the reference energy
//!   [`REFERENCE_ENERGY`] (`k_B·T` at `T* = 1`, about 120 K): where it
//!   turns, or the edge of the force cap's core if it turns inside it (deep
//!   wells such as NH₃'s accelerate collisions that far). Collisions of
//!   light species (H₂) are faster than any oscillation in their shallow
//!   wells. This part depends on temperature; much hotter runs may need a
//!   smaller step than the limit.
//!
//! and `μ` the reduced mass of the two interacting points. Molecules (#406)
//! interact through their sites: a site at lever `L` on a molecule of mass
//! `M` and inertia `I` moves as a mass `1/(1/M + L²/I)` when pushed across
//! the axis, which brings in the rotation. Water (#349) adds its hydrogen
//! bond: the distance Gaussian (`k = ε_HB / w_r²`, with `μ`) and the arm
//! alignment, whose largest angular curvature, once per arm (each holds a
//! bond, and all turn together), acts on the molecule's inertia (the
//! alignment is flat at its optimum, so its maximum, not its
//! minimum, counts).
//!
//! Not covered: collective modes, such as librations of molecules locked
//! in a dense phase (CO₂'s energy error at the default step is about 1 %),
//! and the force cap's `core_stiffness` beyond the reference energy.

use crate::objects2d::ball_impl::pair_potential::{
    CapSettings, CappedPotential, LennardJones, PairLaw, PairPotential, Potential,
};
use crate::objects2d::ball_impl::species::{Species, SpeciesId, SpeciesSet, SpeciesTable};
use crate::objects2d::ball_impl::water::MbWater;

/// Steps per period of the fastest motion at the limit.
pub(crate) const STEPS_PER_PERIOD: f32 = 20.0;
/// `ω·dt` at the limit: `2π / STEPS_PER_PERIOD` ≈ 0.31.
pub(crate) const STEP_MARGIN: f32 = std::f32::consts::TAU / STEPS_PER_PERIOD;
/// Collision energy for the stiffness of the repulsive core: `k_B·T` at
/// `T* = 1` (`ε_Ar`, about 120 K).
pub(crate) const REFERENCE_ENERGY: f32 = 1.0;

/// The largest stable time step of every species pair.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimeStepLimits {
    /// `STEP_MARGIN / ω` per species pair, row-major.
    limits: Vec<f32>,
    species_count: usize,
}

/// One point a species interacts through: an atom's centre or a site of a
/// molecule, with its Lennard-Jones parameters and the mass it moves with.
#[derive(Clone, Copy, Debug)]
struct Point {
    sigma: f32,
    epsilon: f32,
    mass: f32,
}

impl TimeStepLimits {
    /// The limits of `table` with the pair potentials `pairs` (atom pairs,
    /// with their overrides) and the cap `cap` (pairs with a molecule's
    /// sites, as `SiteTable` builds them).
    pub(crate) fn new(table: &SpeciesTable, pairs: &Potential, cap: CapSettings) -> Self {
        let n = table.len();
        let points: Vec<Vec<Point>> = table.iter().map(points_of).collect();
        let mut limits = vec![f32::INFINITY; n * n];
        for a in table.ids() {
            for b in table.ids().filter(|&b| b >= a) {
                let (sa, sb) = (table.species(a), table.species(b));
                let mut omega_sq: f32 = 0.0;
                if sa.molecule.is_none() && sb.molecule.is_none() {
                    let mu = reduced_mass(sa.mass, sb.mass);
                    omega_sq = pairs.stiffness(a, b, REFERENCE_ENERGY) / mu;
                } else {
                    for p in &points[a.index()] {
                        for q in &points[b.index()] {
                            let law = LennardJones {
                                sigma: 0.5 * (p.sigma + q.sigma),
                                epsilon: (p.epsilon * q.epsilon).sqrt(),
                            };
                            let k = stiffness(&CappedPotential::new(law, cap), REFERENCE_ENERGY);
                            omega_sq = omega_sq.max(k / reduced_mass(p.mass, q.mass));
                        }
                    }
                }
                if a == b
                    && let Some(water) = &sa.water
                {
                    omega_sq = omega_sq.max(hydrogen_bond_omega_sq(water, sa));
                }
                let limit = if omega_sq > 0.0 {
                    STEP_MARGIN / omega_sq.sqrt()
                } else {
                    f32::INFINITY
                };
                limits[a.index() * n + b.index()] = limit;
                limits[b.index() * n + a.index()] = limit;
            }
        }
        Self {
            limits,
            species_count: n,
        }
    }

    /// The limit of the pair `a`, `b`.
    pub(crate) fn pair(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        self.limits[a.index() * self.species_count + b.index()]
    }

    /// The smallest limit among the pairs of `present` and the species it
    /// is reported for: of a mixed pair the one with the smaller limit of
    /// its own, on a tie the first in table order. `None` if nothing is
    /// present or nothing has a finite limit.
    pub(crate) fn limit(&self, present: SpeciesSet) -> Option<(f32, SpeciesId)> {
        let mut best: Option<(f32, f32, SpeciesId)> = None;
        for a in present.iter() {
            let own = self.pair(a, a);
            for b in present.iter() {
                let limit = self.pair(a, b);
                if !limit.is_finite() {
                    continue;
                }
                if best.is_none_or(|(l, o, _)| limit < l || (limit == l && own < o)) {
                    best = Some((limit, own, a));
                }
            }
        }
        best.map(|(limit, _, id)| (limit, id))
    }
}

/// The interaction points of `species`.
fn points_of(species: &Species) -> Vec<Point> {
    match &species.molecule {
        None => vec![Point {
            sigma: species.sigma,
            epsilon: species.epsilon,
            mass: species.mass,
        }],
        Some(molecule) => {
            let (mass, inertia) = (molecule.mass(), molecule.inertia());
            molecule
                .sites
                .iter()
                .map(|site| Point {
                    sigma: site.sigma,
                    epsilon: site.epsilon,
                    mass: 1.0 / (1.0 / mass + site.offset * site.offset / inertia),
                })
                .collect()
        }
    }
}

fn reduced_mass(a: f32, b: f32) -> f32 {
    a * b / (a + b)
}

/// The largest curvature `U''` of `potential` that matters for the step:
/// at its well minimum, or along the path of a head-on collision with
/// `energy`. The law's curvature falls outward on the repulsive side, so a
/// collision meets its stiffest point where it turns, or, if it turns
/// inside the capped core, at the core's edge, which it passed.
pub(crate) fn stiffness<L: PairLaw>(potential: &CappedPotential<L>, energy: f32) -> f32 {
    if potential.law().is_null() {
        return 0.0;
    }
    let minimum = potential.law().equilibrium_distance();
    let turn = turning_point(potential, energy);
    let collision = potential
        .curvature(turn)
        .max(potential.curvature(turn.max(potential.r_cap())));
    potential.curvature(minimum).max(collision)
}

/// Centre distance below the minimum where the potential reaches `energy`
/// (bisection; the capped core grows without bound).
fn turning_point<L: PairLaw>(potential: &CappedPotential<L>, energy: f32) -> f32 {
    let (mut inside, mut outside) = (0.0_f32, potential.law().equilibrium_distance());
    for _ in 0..60 {
        let mid = 0.5 * (inside + outside);
        if potential.potential(mid) > energy {
            inside = mid;
        } else {
            outside = mid;
        }
    }
    outside
}

/// `ω²` of the hydrogen bonds of `water` balls of `species`: the distance
/// Gaussian on the reduced mass, and the largest curvature of an arm's
/// alignment Gaussian `-ε_HB·exp(-(cos φ - 1)²/(2 w²))` in the angle `φ`,
/// once per arm ([`HB_ARMS`]: all bonds of a molecule turn with it), on its
/// inertia.
fn hydrogen_bond_omega_sq(water: &MbWater, species: &Species) -> f32 {
    let radial = water.epsilon_hb / (water.width_r * water.width_r);
    let mut omega_sq = radial / reduced_mass(species.mass, species.mass);
    if let Some(inertia) = species.inertia {
        let w = f64::from(water.width_angle);
        let alignment = |phi: f64| -(-(phi.cos() - 1.0).powi(2) / (2.0 * w * w)).exp();
        // Sampled finely enough for the narrowest feature (about √w wide).
        let h = 1e-3;
        let largest = (1..3000)
            .map(|i| {
                let phi = f64::from(i) * h;
                ((alignment(phi + h) - 2.0 * alignment(phi) + alignment(phi - h)) / (h * h)).abs()
            })
            .fold(0.0, f64::max);
        omega_sq = omega_sq.max(HB_ARMS * water.epsilon_hb * largest as f32 / inertia);
    }
    omega_sq
}

/// Arms of an MB water molecule: up to one hydrogen bond each, and their
/// angular stiffnesses add when the molecule turns.
const HB_ARMS: f32 = 3.0;
