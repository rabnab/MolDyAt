// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pair potentials: the interaction law between two balls, separated from the
//! force machinery.
//!
//! A [`PairLaw`] is the bare law (Lennard-Jones, ...). [`CappedPotential`] adds
//! the force cap below `r_cap` and the cutoff shift to any law, so both are
//! implemented once. A [`PairTable`] holds one capped potential per pair of
//! species, mixed from the species parameters. [`Potential`] lists the laws the
//! simulation can use.
//!
//! All distances are centre distances. The SIMD kernel evaluates four pairs at
//! once whose parameters may differ: [`PairLaw::lanes`] and
//! [`CappedLanes::gather`] load them into one lane each.

use crate::objects2d::ball_impl::species::{Species, SpeciesId, SpeciesTable};
use std::simd::Select;
use std::simd::f32x4;
use std::simd::prelude::{SimdFloat, SimdPartialOrd};

/// `σ` of the reference species: the skin is a fixed share of its cutoff.
pub(crate) const LJ_SIG: f32 = crate::objects2d::ball_impl::species::N2_SIGMA;
const LJ_EPS: f32 = 1.;
const SQRT6_OF_TWO: f32 = 1.122_462; // == 2^(1/6)

const MAX_FORCE: f32 = 100.0; // max repulsive force to cap at
/// Slope of the repulsion below `r_cap`: the force grows by this much per unit
/// of extra compression, so deeper overlaps push harder than shallow ones.
/// Stability: smallest reduced mass 0.5 gives ω·dt ≈ 0.2 at dt = 0.01.
const CORE_STIFFNESS: f32 = 200.0;
const CUTOFF_FACTOR: f32 = 4.0;

/// Force cap and cutoff settings of a [`CappedPotential`], fixed when it is built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapSettings {
    /// Force at `r_cap`, where the sloped core starts.
    pub max_force: f32,
    /// Force slope below `r_cap`.
    pub core_stiffness: f32,
    /// Cutoff as a multiple of the law's length scale (σ).
    pub cutoff_factor: f32,
}

impl Default for CapSettings {
    fn default() -> Self {
        CapSettings {
            max_force: MAX_FORCE,
            core_stiffness: CORE_STIFFNESS,
            cutoff_factor: CUTOFF_FACTOR,
        }
    }
}

/// A pair interaction as the force kernel sees it: force magnitude (positive =
/// repulsive) and potential energy at centre distance `r`, zero at and beyond
/// [`Self::cutoff`].
pub trait PairPotential {
    fn force(&self, r: f32) -> f32;
    fn potential(&self, r: f32) -> f32;
    fn force_simd(&self, r: f32x4) -> f32x4;
    fn potential_simd(&self, r: f32x4) -> f32x4;
    fn cutoff(&self) -> f32;
}

/// A bare interaction law, without cap or cutoff.
///
/// `force` must equal `-dU/dr` of `potential`, and fall monotonically across
/// [`Self::cap_search_range`], so that the cap distance is unique and inside it.
pub trait PairLaw: Copy {
    /// The law for four pairs at once, one per SIMD lane.
    type Lanes: LawLanes;

    /// Load four laws (of the same kind, possibly different parameters) into
    /// one lane each.
    fn lanes(laws: [&Self; 4]) -> Self::Lanes;

    fn force(&self, r: f32) -> f32;
    /// `dF/dr`, used by the Newton solve for `r_cap`.
    fn force_derivative(&self, r: f32) -> f32;
    fn potential(&self, r: f32) -> f32;
    /// Length scale (σ for Lennard-Jones); the cutoff is a multiple of it.
    fn length_scale(&self) -> f32;
    /// Distance of the potential minimum, where the force is zero.
    fn equilibrium_distance(&self) -> f32;
    /// Interval that contains the cap distance for any positive cap: the force
    /// falls monotonically across it and is below every cap at its upper end.
    fn cap_search_range(&self) -> (f32, f32) {
        (0.1 * self.length_scale(), self.equilibrium_distance())
    }

    /// [`Self::force`] for four distances, with this law in every lane.
    fn force_simd(&self, r: f32x4) -> f32x4 {
        Self::lanes([self; 4]).force(r)
    }

    /// [`Self::potential`] for four distances, with this law in every lane.
    fn potential_simd(&self, r: f32x4) -> f32x4 {
        Self::lanes([self; 4]).potential(r)
    }
}

/// A [`PairLaw`] for four pairs, one per lane. Lane `i` must give the same bits
/// as the scalar law of pair `i`.
pub trait LawLanes: Copy {
    fn force(&self, r: f32x4) -> f32x4;
    fn potential(&self, r: f32x4) -> f32x4;
}

/// Lanes that evaluate the scalar law one lane at a time: correct for any law,
/// for laws without a vectorised form.
#[derive(Clone, Copy)]
pub struct ScalarLanes<L>(pub [L; 4]);

impl<L: PairLaw> LawLanes for ScalarLanes<L> {
    fn force(&self, r: f32x4) -> f32x4 {
        let r = r.to_array();
        f32x4::from_array(std::array::from_fn(|i| self.0[i].force(r[i])))
    }

    fn potential(&self, r: f32x4) -> f32x4 {
        let r = r.to_array();
        f32x4::from_array(std::array::from_fn(|i| self.0[i].potential(r[i])))
    }
}

/// Lennard-Jones: U(r) = 4·ε·[(σ/r)^12 - (σ/r)^6], minimum -ε at 2^(1/6)·σ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LennardJones {
    pub sigma: f32,
    pub epsilon: f32,
}

impl Default for LennardJones {
    fn default() -> Self {
        LennardJones {
            sigma: LJ_SIG,
            epsilon: LJ_EPS,
        }
    }
}

impl LennardJones {
    /// Lorentz–Berthelot mixing: `σ_ij = (σ_i + σ_j) / 2`, `ε_ij = √(ε_i·ε_j)`.
    pub fn mixed(a: &Species, b: &Species) -> Self {
        LennardJones {
            sigma: 0.5 * (a.sigma + b.sigma),
            epsilon: (a.epsilon * b.epsilon).sqrt(),
        }
    }
}

/// [`LennardJones`] with one `σ`, `ε` per lane.
#[derive(Clone, Copy, Debug)]
pub struct LennardJonesLanes {
    sigma: f32x4,
    epsilon: f32x4,
}

impl PairLaw for LennardJones {
    type Lanes = LennardJonesLanes;

    #[inline]
    fn lanes(laws: [&Self; 4]) -> LennardJonesLanes {
        LennardJonesLanes {
            sigma: f32x4::from_array(laws.map(|l| l.sigma)),
            epsilon: f32x4::from_array(laws.map(|l| l.epsilon)),
        }
    }

    #[inline]
    fn force(&self, r: f32) -> f32 {
        let quot = self.sigma / r;
        let quot_six = quot * quot * quot * quot * quot * quot;
        24. * self.epsilon * quot_six * (2. * quot_six - 1.) / r
    }

    fn force_derivative(&self, r: f32) -> f32 {
        let quot = self.sigma / r;
        let quot_six = quot * quot * quot * quot * quot * quot;
        let quot_twelve = quot_six * quot_six;
        -24.0 * self.epsilon * (26.0 * quot_twelve - 7.0 * quot_six) / (r * r)
    }

    #[inline]
    fn potential(&self, r: f32) -> f32 {
        let quot = self.sigma / r;
        let quot_six = quot * quot * quot * quot * quot * quot;
        4. * self.epsilon * quot_six * (quot_six - 1.)
    }

    fn length_scale(&self) -> f32 {
        self.sigma
    }

    fn equilibrium_distance(&self) -> f32 {
        SQRT6_OF_TWO * self.sigma
    }
}

impl LawLanes for LennardJonesLanes {
    #[inline]
    fn force(&self, r: f32x4) -> f32x4 {
        let quot = self.sigma / r;
        let quot_six = quot * quot * quot * quot * quot * quot;
        f32x4::splat(24.)
            * self.epsilon
            * quot_six
            * (f32x4::splat(2.) * quot_six - f32x4::splat(1.))
            / r
    }

    #[inline]
    fn potential(&self, r: f32x4) -> f32x4 {
        let quot = self.sigma / r;
        let quot_six = quot * quot * quot * quot * quot * quot;
        f32x4::splat(4.) * self.epsilon * quot_six * (quot_six - f32x4::splat(1.))
    }
}

/// A [`PairLaw`] with the force cap and the cutoff shift.
///
/// - Below `r_cap`, where the law's force reaches `max_force`, the force is
///   `max_force + core_stiffness·(r_cap - r)` with the matching potential
///   `U(r) = U(r_cap) - max_force·(r - r_cap) + ½·core_stiffness·(r - r_cap)²`.
///   The slope keeps deep overlaps pushing harder than shallow ones; a flat cap
///   lets crowded neighbours pin overlapping pairs (#240).
/// - At and beyond `cutoff` force and potential are zero; inside, the potential
///   is shifted by `U(cutoff)`, so a pair crossing the cutoff changes no energy.
#[derive(Clone, Debug)]
pub struct CappedPotential<L> {
    law: L,
    settings: CapSettings,
    cutoff: f32,
    r_cap: f32,
    pot_at_cap: f32,
    pot_at_cutoff: f32,
}

impl<L: PairLaw> CappedPotential<L> {
    pub fn new(law: L, settings: CapSettings) -> Self {
        let r_cap = force_cap_distance(&law, settings.max_force);
        let cutoff = settings.cutoff_factor * law.length_scale();
        CappedPotential {
            settings,
            cutoff,
            r_cap,
            pot_at_cap: law.potential(r_cap),
            pot_at_cutoff: law.potential(cutoff),
            law,
        }
    }

    pub fn law(&self) -> &L {
        &self.law
    }

    pub fn settings(&self) -> &CapSettings {
        &self.settings
    }

    /// Distance below which the force follows the sloped core.
    pub fn r_cap(&self) -> f32 {
        self.r_cap
    }
}

impl<L: PairLaw> PairPotential for CappedPotential<L> {
    #[inline]
    fn force(&self, r: f32) -> f32 {
        if r >= self.cutoff {
            return 0.;
        }
        self.law.force(r.max(self.r_cap)) + self.settings.core_stiffness * (self.r_cap - r).max(0.)
    }

    #[inline]
    fn potential(&self, r: f32) -> f32 {
        if r >= self.cutoff {
            return 0.;
        }
        let potential = if r < self.r_cap {
            let compression = self.r_cap - r;
            self.pot_at_cap
                + compression
                    * (self.settings.max_force + 0.5 * self.settings.core_stiffness * compression)
        } else {
            self.law.potential(r)
        };
        potential - self.pot_at_cutoff
    }

    #[inline]
    fn force_simd(&self, r: f32x4) -> f32x4 {
        CappedLanes::gather([self; 4]).force(r)
    }

    #[inline]
    fn potential_simd(&self, r: f32x4) -> f32x4 {
        CappedLanes::gather([self; 4]).potential(r)
    }

    #[inline]
    fn cutoff(&self) -> f32 {
        self.cutoff
    }
}

/// Four [`CappedPotential`]s, one per lane, for the SIMD force kernel.
#[derive(Clone, Copy)]
pub struct CappedLanes<LL> {
    law: LL,
    cutoff: f32x4,
    r_cap: f32x4,
    pot_at_cap: f32x4,
    pot_at_cutoff: f32x4,
    max_force: f32x4,
    core_stiffness: f32x4,
}

impl<LL: LawLanes> CappedLanes<LL> {
    /// Load four pair potentials into one lane each.
    #[inline]
    pub fn gather<L: PairLaw<Lanes = LL>>(pairs: [&CappedPotential<L>; 4]) -> Self {
        CappedLanes {
            law: L::lanes(pairs.map(|p| &p.law)),
            cutoff: f32x4::from_array(pairs.map(|p| p.cutoff)),
            r_cap: f32x4::from_array(pairs.map(|p| p.r_cap)),
            pot_at_cap: f32x4::from_array(pairs.map(|p| p.pot_at_cap)),
            pot_at_cutoff: f32x4::from_array(pairs.map(|p| p.pot_at_cutoff)),
            max_force: f32x4::from_array(pairs.map(|p| p.settings.max_force)),
            core_stiffness: f32x4::from_array(pairs.map(|p| p.settings.core_stiffness)),
        }
    }

    #[inline]
    fn inside_cutoff(&self, r: f32x4) -> std::simd::Mask<i32, 4> {
        r.simd_lt(self.cutoff)
    }

    /// Force of each lane's pair at its distance in `r`; see
    /// [`CappedPotential`].
    #[inline]
    pub fn force(&self, r: f32x4) -> f32x4 {
        // The law is evaluated at max(r, r_cap) only, so it stays finite in the core.
        let r_outer = r.simd_max(self.r_cap);
        let compression = (r_outer - r).simd_max(f32x4::splat(0.));
        let force = self.law.force(r_outer) + self.core_stiffness * compression;
        self.inside_cutoff(r).select(force, f32x4::splat(0.))
    }

    /// Potential of each lane's pair at its distance in `r`.
    #[inline]
    pub fn potential(&self, r: f32x4) -> f32x4 {
        let potential_outer = self.law.potential(r.simd_max(self.r_cap));
        let compression = self.r_cap - r;
        let potential_core = self.pot_at_cap
            + compression
                * (self.max_force + f32x4::splat(0.5) * self.core_stiffness * compression);
        let potential_shifted = r
            .simd_lt(self.r_cap)
            .select(potential_core, potential_outer)
            - self.pot_at_cutoff;
        self.inside_cutoff(r)
            .select(potential_shifted, f32x4::splat(0.))
    }
}

/// Distance `r_cap` at which the law's force equals `max_force`: Newton's
/// method, with a bisection as fallback.
fn force_cap_distance<L: PairLaw>(law: &L, max_force: f32) -> f32 {
    newton_cap_distance(law, max_force).unwrap_or_else(|| {
        log::warn!("r_cap: Newton found no valid root for max_force {max_force}, bisecting");
        bisect_cap_distance(law, max_force)
    })
}

/// Newton's method from `r = σ/2`; `None` if it does not converge, or if it
/// converges to a point that is not a valid cap distance (outside the search
/// range, or where the force is not `max_force`).
pub(crate) fn newton_cap_distance<L: PairLaw>(law: &L, max_force: f32) -> Option<f32> {
    const MAX_ITERATIONS: usize = 20;
    const CONVERGENCE_THRESHOLD: f32 = 1e-6;
    const FORCE_TOLERANCE: f32 = 1e-4;

    let mut r = 0.5 * law.length_scale();
    for _ in 0..MAX_ITERATIONS {
        let delta = (law.force(r) - max_force) / law.force_derivative(r);
        r -= delta;
        if delta.abs() < CONVERGENCE_THRESHOLD * r.abs() {
            let (_, r_high) = law.cap_search_range();
            let in_core = r > 0.0 && r < r_high;
            let hits_cap = (law.force(r) - max_force).abs() <= FORCE_TOLERANCE * max_force;
            return (r.is_finite() && in_core && hits_cap).then_some(r);
        }
    }
    None
}

/// Bisection over [`PairLaw::cap_search_range`], which contains the root for
/// any positive `max_force`.
pub(crate) fn bisect_cap_distance<L: PairLaw>(law: &L, max_force: f32) -> f32 {
    const ITERATIONS: usize = 50;

    let (mut r_low, mut r_high) = law.cap_search_range();
    for _ in 0..ITERATIONS {
        let r_mid = (r_low + r_high) / 2.0;
        if law.force(r_mid) > max_force {
            r_low = r_mid;
        } else {
            r_high = r_mid;
        }
    }
    (r_low + r_high) / 2.0
}

/// One [`CappedPotential`] per ordered pair of species, built once from the
/// species table. Symmetric: the potential of `(a, b)` equals that of `(b, a)`.
#[derive(Clone, Debug)]
pub struct PairTable<L> {
    species_count: usize,
    settings: CapSettings,
    /// Row-major: pair `(a, b)` at `a * species_count + b`.
    pairs: Vec<CappedPotential<L>>,
}

impl<L: PairLaw> PairTable<L> {
    /// The table for `species`, with each pair's law from `law_for`.
    pub fn new(
        species: &SpeciesTable,
        settings: CapSettings,
        law_for: impl Fn(&Species, &Species) -> L,
    ) -> Self {
        let pairs = species
            .iter()
            .flat_map(|a| species.iter().map(move |b| (a, b)))
            .map(|(a, b)| CappedPotential::new(law_for(a, b), settings))
            .collect();
        PairTable {
            species_count: species.len(),
            settings,
            pairs,
        }
    }

    pub fn species_count(&self) -> usize {
        self.species_count
    }

    pub fn settings(&self) -> &CapSettings {
        &self.settings
    }

    #[inline]
    pub fn pair(&self, a: SpeciesId, b: SpeciesId) -> &CappedPotential<L> {
        &self.pairs[a.index() * self.species_count + b.index()]
    }

    /// The potentials of species `a` with every species, indexed by the other
    /// species: what the force kernel reads for one ball.
    #[inline]
    pub fn row(&self, a: SpeciesId) -> &[CappedPotential<L>] {
        let start = a.index() * self.species_count;
        &self.pairs[start..start + self.species_count]
    }
}

/// The interaction laws the simulation can use, each as a table over the
/// species pairs.
#[derive(Clone, Debug)]
pub enum Potential {
    LennardJones(PairTable<LennardJones>),
}

impl Potential {
    /// Lennard-Jones between all species, mixed by Lorentz–Berthelot.
    pub fn lennard_jones(species: &SpeciesTable, settings: CapSettings) -> Self {
        Potential::LennardJones(PairTable::new(species, settings, LennardJones::mixed))
    }

    pub fn cap_settings(&self) -> &CapSettings {
        match self {
            Potential::LennardJones(t) => t.settings(),
        }
    }

    pub fn species_count(&self) -> usize {
        match self {
            Potential::LennardJones(t) => t.species_count(),
        }
    }

    pub fn r_cap(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        match self {
            Potential::LennardJones(t) => t.pair(a, b).r_cap(),
        }
    }

    pub fn cutoff(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        match self {
            Potential::LennardJones(t) => t.pair(a, b).cutoff(),
        }
    }

    /// Centre distance of the potential minimum of the pair.
    pub fn equilibrium_distance(&self, a: SpeciesId, b: SpeciesId) -> f32 {
        match self {
            Potential::LennardJones(t) => t.pair(a, b).law().equilibrium_distance(),
        }
    }

    pub fn force(&self, a: SpeciesId, b: SpeciesId, r: f32) -> f32 {
        match self {
            Potential::LennardJones(t) => t.pair(a, b).force(r),
        }
    }

    pub fn potential(&self, a: SpeciesId, b: SpeciesId, r: f32) -> f32 {
        match self {
            Potential::LennardJones(t) => t.pair(a, b).potential(r),
        }
    }
}
