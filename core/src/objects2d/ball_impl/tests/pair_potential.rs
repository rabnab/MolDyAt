// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::ball_impl::pair_potential::{
    CapSettings, CappedPotential, LennardJones, PairLaw, PairPotential, ScalarLanes,
    bisect_cap_distance, newton_cap_distance,
};
use std::simd::f32x4;

fn approx(a: f32, b: f32, eps: f32, what: &str) {
    assert!(
        (a - b).abs() <= eps * (1.0 + a.abs().max(b.abs())),
        "{what}: {a} vs {b} with eps {eps}"
    );
}

/// Purely repulsive r^-12 law, to show the cap and cutoff work for any law.
#[derive(Clone, Copy)]
struct RepulsiveCore {
    sigma: f32,
    epsilon: f32,
}

impl PairLaw for RepulsiveCore {
    type Lanes = ScalarLanes<Self>;

    fn lanes(laws: [&Self; 4]) -> Self::Lanes {
        ScalarLanes(laws.map(|l| *l))
    }

    fn force(&self, r: f32) -> f32 {
        48. * self.epsilon * (self.sigma / r).powi(12) / r
    }

    fn force_derivative(&self, r: f32) -> f32 {
        -624. * self.epsilon * (self.sigma / r).powi(12) / (r * r)
    }

    fn potential(&self, r: f32) -> f32 {
        4. * self.epsilon * (self.sigma / r).powi(12)
    }

    fn length_scale(&self) -> f32 {
        self.sigma
    }

    /// No minimum: the force only vanishes as r → ∞.
    fn equilibrium_distance(&self) -> f32 {
        f32::INFINITY
    }

    /// At 2σ the force is ~0.006·ε/σ, below any cap.
    fn cap_search_range(&self) -> (f32, f32) {
        (0.1 * self.sigma, 2. * self.sigma)
    }
}

/// Lennard-Jones whose derivative is NaN, so Newton never converges.
#[derive(Clone, Copy)]
struct NewtonFails(LennardJones);

impl PairLaw for NewtonFails {
    type Lanes = ScalarLanes<Self>;

    fn lanes(laws: [&Self; 4]) -> Self::Lanes {
        ScalarLanes(laws.map(|l| *l))
    }

    fn force(&self, r: f32) -> f32 {
        self.0.force(r)
    }

    fn force_derivative(&self, _r: f32) -> f32 {
        f32::NAN
    }

    fn potential(&self, r: f32) -> f32 {
        self.0.potential(r)
    }

    fn length_scale(&self) -> f32 {
        self.0.length_scale()
    }

    fn equilibrium_distance(&self) -> f32 {
        self.0.equilibrium_distance()
    }
}

/// Lennard-Jones whose derivative sends the first Newton step to `target`
/// (or nowhere, if `None`) and then stalls Newton there, so it "converges" to a
/// point that is not the cap distance.
#[derive(Clone, Copy)]
struct NewtonMisleads {
    lj: LennardJones,
    max_force: f32,
    target: Option<f32>,
}

impl PairLaw for NewtonMisleads {
    type Lanes = ScalarLanes<Self>;

    fn lanes(laws: [&Self; 4]) -> Self::Lanes {
        ScalarLanes(laws.map(|l| *l))
    }

    fn force(&self, r: f32) -> f32 {
        self.lj.force(r)
    }

    fn force_derivative(&self, r: f32) -> f32 {
        let start = 0.5 * self.lj.sigma;
        match self.target {
            Some(target) if r == start => (self.force(r) - self.max_force) / (r - target),
            _ => 1e30,
        }
    }

    fn potential(&self, r: f32) -> f32 {
        self.lj.potential(r)
    }

    fn length_scale(&self) -> f32 {
        self.lj.length_scale()
    }

    fn equilibrium_distance(&self) -> f32 {
        self.lj.equilibrium_distance()
    }
}

/// Includes caps below F_LJ(σ) = 24·ε/σ (the drift suite's 10), where the
/// cap distance lies above σ.
const MAX_FORCES: [f32; 5] = [10.0, 24.0, 50.0, 100.0, 200.0];

fn lennard_jones_cases() -> [LennardJones; 5] {
    [(1.0, 1.0), (0.5, 1.0), (2.0, 1.0), (1.0, 0.5), (1.0, 2.0)]
        .map(|(sigma, epsilon)| LennardJones { sigma, epsilon })
}

fn capped<L: PairLaw>(law: L, max_force: f32) -> CappedPotential<L> {
    let settings = CapSettings {
        max_force,
        ..CapSettings::default()
    };
    CappedPotential::new(law, settings)
}

/// Calls `check(potential, max_force)` for every Lennard-Jones case and the
/// repulsive test law, at every cap in `MAX_FORCES`.
macro_rules! for_each_potential {
    ($check:ident) => {
        for max_force in MAX_FORCES {
            for lj in lennard_jones_cases() {
                $check(&capped(lj, max_force), max_force);
            }
            let core = RepulsiveCore {
                sigma: 1.0,
                epsilon: 1.0,
            };
            $check(&capped(core, max_force), max_force);
        }
    };
}

/// Distances from deep overlap through the core, `r_cap`, the outer law and
/// the cutoff to beyond it (three SIMD chunks).
fn probe_distances<L: PairLaw>(p: &CappedPotential<L>) -> [f32; 12] {
    let sigma = p.law().length_scale();
    let r_cap = p.r_cap();
    let cutoff = p.cutoff();
    [
        -1.2 * sigma,
        -0.4 * sigma,
        0.0,
        0.5 * r_cap,
        r_cap,
        1.01 * r_cap,
        sigma,
        p.law().cap_search_range().1,
        0.5 * cutoff,
        cutoff - 1e-3 * sigma,
        cutoff,
        cutoff + 0.1 * sigma,
    ]
}

fn check_simd_matches_scalar<L: PairLaw>(p: &CappedPotential<L>, _max_force: f32) {
    let dists = probe_distances(p);
    for (chunk_idx, chunk) in dists.as_chunks::<4>().0.iter().enumerate() {
        let force = p.force_simd(f32x4::from(*chunk)).to_array();
        let potential = p.potential_simd(f32x4::from(*chunk)).to_array();
        for (i, &r) in chunk.iter().enumerate() {
            let at = format!("r = {r} (chunk {chunk_idx})");
            assert_eq!(force[i].to_bits(), p.force(r).to_bits(), "force at {at}");
            assert_eq!(
                potential[i].to_bits(),
                p.potential(r).to_bits(),
                "potential at {at}"
            );
        }
    }
}

#[test]
fn test_capped_potential_simd_matches_scalar_bit_for_bit() {
    for_each_potential!(check_simd_matches_scalar);
}

/// The core force starts at `max_force`, the slope of the core potential, so
/// F = -dU/dr holds below `r_cap` too.
fn check_force_at_r_cap_is_max_force<L: PairLaw>(p: &CappedPotential<L>, max_force: f32) {
    let force = p.law().force(p.r_cap());
    assert!(
        (force - max_force).abs() <= 1e-4 * max_force,
        "F(r_cap) = {force} for max_force {max_force}"
    );
    assert!(p.r_cap() > 0.0 && p.r_cap() < p.law().cap_search_range().1);
}

#[test]
fn test_capped_potential_force_at_r_cap_is_max_force() {
    for_each_potential!(check_force_at_r_cap_is_max_force);
}

/// F = -dU/dr on both sides of `r_cap` and across the core. A jump in U at
/// `r_cap` would show up as a slope error in the one-sided differences there.
fn check_energy_continuity_at_r_cap<L: PairLaw>(p: &CappedPotential<L>, _max_force: f32) {
    let sigma = p.law().length_scale();
    let r_cap = p.r_cap();
    let h = 1e-3 * sigma;

    let below = (p.potential(r_cap) - p.potential(r_cap - h)) / h;
    approx(
        p.force(r_cap - 0.5 * h),
        -below,
        1e-2,
        "slope just below r_cap",
    );
    let above = (p.potential(r_cap + h) - p.potential(r_cap)) / h;
    approx(
        p.force(r_cap + 0.5 * h),
        -above,
        1e-2,
        "slope just above r_cap",
    );

    for r in [-0.4 * sigma, 0.0, 0.5 * r_cap, 0.9 * r_cap, 1.1 * r_cap] {
        let slope = (p.potential(r + h) - p.potential(r - h)) / (2. * h);
        approx(p.force(r), -slope, 1e-2, &format!("slope at r = {r}"));
    }
}

#[test]
fn test_capped_potential_energy_continuity_at_r_cap() {
    for_each_potential!(check_energy_continuity_at_r_cap);
}

fn check_energy_continuity_at_cutoff<L: PairLaw>(p: &CappedPotential<L>, _max_force: f32) {
    let sigma = p.law().length_scale();
    let cutoff = p.cutoff();
    for r in [cutoff, cutoff + 0.1 * sigma, 10.0 * cutoff] {
        assert_eq!(p.force(r), 0.0, "force at {r}");
        assert_eq!(p.potential(r), 0.0, "potential at {r}");
    }
    // Just inside, U(cutoff - h) = ∫F ≈ F(cutoff - h/2)·h: no jump when a pair
    // crosses the cutoff. The midpoint rule errs by O(h²) relative, far below 1 %.
    let h = 0.05 * sigma;
    let inside = p.potential(cutoff - h);
    let expected = p.force(cutoff - 0.5 * h) * h;
    assert!(
        (inside - expected).abs() <= 0.01 * expected.abs(),
        "U just inside the cutoff: {inside} vs {expected}"
    );
}

#[test]
fn test_capped_potential_energy_continuity_at_cutoff() {
    for_each_potential!(check_energy_continuity_at_cutoff);
}

#[test]
fn test_cutoff_is_cutoff_factor_times_length_scale() {
    let settings = CapSettings::default();
    for lj in lennard_jones_cases() {
        let p = CappedPotential::new(lj, settings);
        assert_eq!(p.cutoff(), settings.cutoff_factor * lj.sigma);
    }
}

#[test]
fn test_newton_converges_for_lennard_jones() {
    for max_force in MAX_FORCES.into_iter().chain([1000.0]) {
        for lj in lennard_jones_cases() {
            assert!(
                newton_cap_distance(&lj, max_force).is_some(),
                "Newton did not converge for {lj:?}, max_force {max_force}"
            );
        }
    }
}

#[test]
fn test_bisection_agrees_with_newton() {
    for max_force in MAX_FORCES {
        for lj in lennard_jones_cases() {
            let newton = newton_cap_distance(&lj, max_force).unwrap();
            let bisection = bisect_cap_distance(&lj, max_force);
            approx(
                bisection,
                newton,
                1e-5,
                &format!("{lj:?}, max_force {max_force}"),
            );
        }
    }
}

#[test]
fn test_cap_distance_falls_back_to_bisection_when_newton_fails() {
    for max_force in MAX_FORCES {
        let law = NewtonFails(LennardJones::default());
        assert_eq!(newton_cap_distance(&law, max_force), None);
        let p = capped(law, max_force);
        assert_eq!(p.r_cap(), bisect_cap_distance(&law, max_force));
        check_force_at_r_cap_is_max_force(&p, max_force);
    }
}

#[test]
fn test_newton_rejects_a_root_that_is_not_the_cap_distance() {
    let lj = LennardJones::default();
    // Stalls at σ/2 (F ≠ max_force), lands at -σ, lands beyond the range.
    for target in [None, Some(-1.0), Some(3.0)] {
        for max_force in MAX_FORCES {
            let law = NewtonMisleads {
                lj,
                max_force,
                target,
            };
            assert_eq!(
                newton_cap_distance(&law, max_force),
                None,
                "target {target:?}, max_force {max_force}"
            );
            let p = capped(law, max_force);
            assert_eq!(p.r_cap(), bisect_cap_distance(&lj, max_force));
            check_force_at_r_cap_is_max_force(&p, max_force);
        }
    }
}
