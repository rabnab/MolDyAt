// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::ball_impl::ball_physics::PhysicsConstants;
use crate::objects2d::ball_impl::pair_potential::{
    CappedPotential, LennardJones, PairLaw, PairPotential,
};
use std::simd::f32x4;

fn approx(a: f32, b: f32, eps: f32, what: &str) {
    assert!(
        (a - b).abs() <= eps * (1.0 + a.abs().max(b.abs())),
        "{what}: {a} vs {b} with eps {eps}"
    );
}

/// Purely repulsive r^-12 law with an exact derivative, so the Newton solve
/// for `r_cap` converges (Lennard-Jones goes through the bisection fallback).
#[derive(Clone, Copy)]
struct RepulsiveCore {
    sigma: f32,
    epsilon: f32,
}

impl PairLaw for RepulsiveCore {
    fn force(&self, r: f32) -> f32 {
        48. * self.epsilon * (self.sigma / r).powi(12) / r
    }

    fn force_derivative(&self, r: f32) -> f32 {
        -624. * self.epsilon * (self.sigma / r).powi(12) / (r * r)
    }

    fn potential(&self, r: f32) -> f32 {
        4. * self.epsilon * (self.sigma / r).powi(12)
    }

    fn force_simd(&self, r: f32x4) -> f32x4 {
        f32x4::from_array(r.to_array().map(|r| self.force(r)))
    }

    fn potential_simd(&self, r: f32x4) -> f32x4 {
        f32x4::from_array(r.to_array().map(|r| self.potential(r)))
    }

    fn length_scale(&self) -> f32 {
        self.sigma
    }

    fn equilibrium_distance(&self) -> f32 {
        self.sigma
    }
}

fn lennard_jones_cases() -> [LennardJones; 5] {
    [(1.0, 1.0), (0.5, 1.0), (2.0, 1.0), (1.0, 0.5), (1.0, 2.0)]
        .map(|(sigma, epsilon)| LennardJones { sigma, epsilon })
}

fn capped<L: PairLaw>(law: L) -> CappedPotential<L> {
    CappedPotential::new(law, &PhysicsConstants::default())
}

/// Calls `check` for every Lennard-Jones case and the repulsive test law.
macro_rules! for_each_potential {
    ($check:ident) => {
        for lj in lennard_jones_cases() {
            $check(&capped(lj));
        }
        $check(&capped(RepulsiveCore {
            sigma: 1.0,
            epsilon: 1.0,
        }));
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
        p.law().equilibrium_distance(),
        0.5 * cutoff,
        cutoff - 1e-3 * sigma,
        cutoff,
        cutoff + 0.1 * sigma,
    ]
}

fn check_simd_matches_scalar<L: PairLaw>(p: &CappedPotential<L>) {
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

fn check_force_at_r_cap_is_max_force<L: PairLaw>(p: &CappedPotential<L>) {
    let max_force = PhysicsConstants::default().max_force;
    approx(p.force(p.r_cap()), max_force, 1e-2, "F(r_cap)");
    assert!(p.r_cap() > 0.0 && p.r_cap() < p.law().length_scale());
}

#[test]
fn test_capped_potential_force_at_r_cap_is_max_force() {
    for_each_potential!(check_force_at_r_cap_is_max_force);
}

/// F = -dU/dr on both sides of `r_cap` and across the core. A jump in U at
/// `r_cap` would show up as a slope error in the one-sided differences there.
fn check_energy_continuity_at_r_cap<L: PairLaw>(p: &CappedPotential<L>) {
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

fn check_energy_continuity_at_cutoff<L: PairLaw>(p: &CappedPotential<L>) {
    let sigma = p.law().length_scale();
    let cutoff = p.cutoff();
    for r in [cutoff, cutoff + 0.1 * sigma, 10.0 * cutoff] {
        assert_eq!(p.force(r), 0.0, "force at {r}");
        assert_eq!(p.potential(r), 0.0, "potential at {r}");
    }
    // Just inside, U ≈ -F·(cutoff - r): no jump when a pair crosses the cutoff.
    let h = 1e-3 * sigma;
    let inside = p.potential(cutoff - h);
    approx(
        inside,
        p.force(cutoff - h) * h,
        1e-5,
        "U just inside the cutoff",
    );
}

#[test]
fn test_capped_potential_energy_continuity_at_cutoff() {
    for_each_potential!(check_energy_continuity_at_cutoff);
}

#[test]
fn test_cutoff_is_cutoff_factor_times_length_scale() {
    let constants = PhysicsConstants::default();
    for lj in lennard_jones_cases() {
        let p = CappedPotential::new(lj, &constants);
        assert_eq!(p.cutoff(), constants.cutoff_factor * lj.sigma);
    }
}
