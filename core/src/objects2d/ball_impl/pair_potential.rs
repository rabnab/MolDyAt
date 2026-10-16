// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pair potentials: the interaction law between two balls, separated from the
//! force machinery.
//!
//! A [`PairLaw`] is the bare law (Lennard-Jones, ...). [`CappedPotential`] adds
//! the force cap below `r_cap` and the cutoff shift to any law, so both are
//! implemented once. [`Potential`] lists the laws the simulation can use.
//!
//! All distances are edge distances (centre distance minus both radii), so
//! Lennard-Jones here acts as a Kihara-type potential whose hard cores are the
//! balls themselves: the edge distance is zero at centre distance `R_i + R_j`.

use std::simd::Select;
use std::simd::f32x4;
use std::simd::prelude::{SimdFloat, SimdPartialOrd};

pub(crate) const LJ_SIG: f32 = 1.;
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
/// repulsive) and potential energy at edge distance `r`, zero at and beyond
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
/// `force` must equal `-dU/dr` of `potential`, and fall monotonically from
/// `0.1·length_scale()` to `equilibrium_distance()`, so that the cap distance
/// is unique and inside that range.
pub trait PairLaw {
    fn force(&self, r: f32) -> f32;
    /// `dF/dr`, used by the Newton solve for `r_cap`.
    fn force_derivative(&self, r: f32) -> f32;
    fn potential(&self, r: f32) -> f32;
    fn force_simd(&self, r: f32x4) -> f32x4;
    fn potential_simd(&self, r: f32x4) -> f32x4;
    /// Length scale (σ for Lennard-Jones); the cutoff is a multiple of it.
    fn length_scale(&self) -> f32;
    /// Distance of the potential minimum, where the force is zero.
    fn equilibrium_distance(&self) -> f32;
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

impl PairLaw for LennardJones {
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

    #[inline]
    fn force_simd(&self, r: f32x4) -> f32x4 {
        let quot = f32x4::splat(self.sigma) / r;
        let quot_six = quot * quot * quot * quot * quot * quot;
        f32x4::splat(24. * self.epsilon)
            * quot_six
            * (f32x4::splat(2.) * quot_six - f32x4::splat(1.))
            / r
    }

    #[inline]
    fn potential_simd(&self, r: f32x4) -> f32x4 {
        let quot = f32x4::splat(self.sigma) / r;
        let quot_six = quot * quot * quot * quot * quot * quot;
        f32x4::splat(4. * self.epsilon) * quot_six * (quot_six - f32x4::splat(1.))
    }

    fn length_scale(&self) -> f32 {
        self.sigma
    }

    fn equilibrium_distance(&self) -> f32 {
        SQRT6_OF_TWO * self.sigma
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

    #[inline]
    fn inside_cutoff(&self, r: f32x4) -> std::simd::Mask<i32, 4> {
        r.simd_lt(f32x4::splat(self.cutoff))
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
        // The law is evaluated at max(r, r_cap) only, so it stays finite in the core.
        let r_outer = r.simd_max(f32x4::splat(self.r_cap));
        let compression = (r_outer - r).simd_max(f32x4::splat(0.));
        let force =
            self.law.force_simd(r_outer) + f32x4::splat(self.settings.core_stiffness) * compression;
        self.inside_cutoff(r).select(force, f32x4::splat(0.))
    }

    #[inline]
    fn potential_simd(&self, r: f32x4) -> f32x4 {
        let r_cap = f32x4::splat(self.r_cap);
        let potential_outer = self.law.potential_simd(r.simd_max(r_cap));
        let compression = r_cap - r;
        let potential_core = f32x4::splat(self.pot_at_cap)
            + compression
                * (f32x4::splat(self.settings.max_force)
                    + f32x4::splat(0.5 * self.settings.core_stiffness) * compression);
        let potential_shifted = r.simd_lt(r_cap).select(potential_core, potential_outer)
            - f32x4::splat(self.pot_at_cutoff);
        self.inside_cutoff(r)
            .select(potential_shifted, f32x4::splat(0.))
    }

    #[inline]
    fn cutoff(&self) -> f32 {
        self.cutoff
    }
}

/// Distance `r_cap` at which the law's force equals `max_force`: Newton's
/// method, with a bisection as fallback.
fn force_cap_distance<L: PairLaw>(law: &L, max_force: f32) -> f32 {
    newton_cap_distance(law, max_force).unwrap_or_else(|| {
        log::warn!("r_cap: Newton did not converge for max_force {max_force}, bisecting");
        bisect_cap_distance(law, max_force)
    })
}

/// Newton's method from `r = σ/2`; `None` if it does not converge.
pub(crate) fn newton_cap_distance<L: PairLaw>(law: &L, max_force: f32) -> Option<f32> {
    const MAX_ITERATIONS: usize = 20;
    const CONVERGENCE_THRESHOLD: f32 = 1e-6;

    let mut r = 0.5 * law.length_scale();
    for _ in 0..MAX_ITERATIONS {
        let delta = (law.force(r) - max_force) / law.force_derivative(r);
        r -= delta;
        if delta.abs() < CONVERGENCE_THRESHOLD * r {
            return Some(r);
        }
    }
    None
}

/// Bisection over `[0.1·σ, equilibrium distance]`: the force falls to zero at
/// the equilibrium distance, so the root lies there for any positive `max_force`.
pub(crate) fn bisect_cap_distance<L: PairLaw>(law: &L, max_force: f32) -> f32 {
    const ITERATIONS: usize = 50;

    let mut r_low = 0.1 * law.length_scale();
    let mut r_high = law.equilibrium_distance();
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

/// The interaction laws the simulation can use.
#[derive(Clone, Debug)]
pub enum Potential {
    LennardJones(CappedPotential<LennardJones>),
}

impl Potential {
    pub fn lennard_jones(lj: LennardJones, settings: CapSettings) -> Self {
        Potential::LennardJones(CappedPotential::new(lj, settings))
    }

    pub fn cap_settings(&self) -> &CapSettings {
        match self {
            Potential::LennardJones(p) => p.settings(),
        }
    }

    pub fn r_cap(&self) -> f32 {
        match self {
            Potential::LennardJones(p) => p.r_cap(),
        }
    }

    pub fn equilibrium_distance(&self) -> f32 {
        match self {
            Potential::LennardJones(p) => p.law().equilibrium_distance(),
        }
    }
}

impl PairPotential for Potential {
    #[inline]
    fn force(&self, r: f32) -> f32 {
        match self {
            Potential::LennardJones(p) => p.force(r),
        }
    }

    #[inline]
    fn potential(&self, r: f32) -> f32 {
        match self {
            Potential::LennardJones(p) => p.potential(r),
        }
    }

    #[inline]
    fn force_simd(&self, r: f32x4) -> f32x4 {
        match self {
            Potential::LennardJones(p) => p.force_simd(r),
        }
    }

    #[inline]
    fn potential_simd(&self, r: f32x4) -> f32x4 {
        match self {
            Potential::LennardJones(p) => p.potential_simd(r),
        }
    }

    #[inline]
    fn cutoff(&self) -> f32 {
        match self {
            Potential::LennardJones(p) => p.cutoff(),
        }
    }
}
