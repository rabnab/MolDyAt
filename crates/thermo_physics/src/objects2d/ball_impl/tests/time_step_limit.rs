// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Time step limits derived from the species (#427).

use crate::application::sim_config::DEFAULT_TIME_STEP;
use crate::objects2d::ball_impl::pair_potential::{
    CapSettings, CappedPotential, LennardJones, PairPotential, Potential,
};
use crate::objects2d::ball_impl::species::{Species, SpeciesSet, SpeciesTable};
use crate::objects2d::ball_impl::time_step_limit::{STEP_MARGIN, TimeStepLimits, stiffness};

fn builtin_limits() -> TimeStepLimits {
    let table = SpeciesTable::builtin();
    let cap = CapSettings::default();
    TimeStepLimits::new(table, &Potential::lennard_jones(table, cap, &[]), cap)
}

fn own_limit(name: &str) -> f32 {
    let id = SpeciesTable::builtin().by_name(name).unwrap();
    builtin_limits().pair(id, id)
}

/// `U''` from finite differences of the force, independent of
/// `CappedPotential::curvature`.
fn numeric_curvature(p: &CappedPotential<LennardJones>, r: f32) -> f32 {
    let h = 1e-3 * r;
    -(p.force(r + h) - p.force(r - h)) / (2.0 * h)
}

/// The curvature the limits use is `-dF/dr`, on the law and in the capped
/// core.
#[test]
fn curvature_is_minus_the_derivative_of_the_force() {
    let p = CappedPotential::new(
        LennardJones {
            sigma: 1.0,
            epsilon: 4.0,
        },
        CapSettings::default(),
    );
    for r in [0.5_f32, 0.8, 0.9, 1.0, 1.12, 1.5, 2.0] {
        let (exact, numeric) = (p.curvature(r), numeric_curvature(&p, r));
        assert!(
            (exact - numeric).abs() <= 2e-2 * exact.abs().max(1.0),
            "r={r}: U'' {exact} vs {numeric}"
        );
    }
}

/// For every built-in single-site species at its limit, `ω·dt` of its like
/// pair stays within the margin, at the well minimum and at the turning
/// point of a collision at `k_B·T = 1`, with `ω` from finite differences.
#[test]
fn omega_times_the_limit_stays_within_the_margin() {
    let table = SpeciesTable::builtin();
    let limits = builtin_limits();
    let cap = CapSettings::default();
    for id in table.ids() {
        let s = table.species(id);
        if s.molecule.is_some() {
            continue;
        }
        let p = CappedPotential::new(
            LennardJones {
                sigma: s.sigma,
                epsilon: s.epsilon,
            },
            cap,
        );
        let mu = 0.5 * s.mass;
        let limit = limits.pair(id, id);
        let well = 2f32.powf(1.0 / 6.0) * s.sigma;
        let mut turn = well;
        while p.potential(turn) < 1.0 {
            turn *= 0.999;
        }
        for r in [well, turn] {
            let omega = (numeric_curvature(&p, r).max(0.0) / mu).sqrt();
            assert!(
                omega * limit <= STEP_MARGIN * 1.02,
                "{} at r={r}: ω·dt = {}",
                s.name,
                omega * limit
            );
        }
    }
}

/// The limits that replaced the hand-set constants (H₂O 0.001, H₂ 0.001,
/// CO₂ 0.0025, NH₃ 0.01), and the species the default step is fine for.
#[test]
fn built_in_limits() {
    for s in SpeciesTable::builtin().iter() {
        println!("{}: {:.5}", s.name, own_limit(&s.name));
    }
    let expect = |name: &str, lo: f32, hi: f32| {
        let limit = own_limit(name);
        assert!(
            (lo..hi).contains(&limit),
            "{name}: {limit} not in {lo}..{hi}"
        );
    };
    expect("H2O", 0.0006, 0.0008); // the alignment of its three bonds
    expect("H2_2S", 0.0013, 0.0019); // hot collisions of light sites
    // Its collisions pass the stiff edge of the capped core (its well alone,
    // ω ≈ 41.5, gave the former 0.01).
    expect("NH3", 0.003, 0.004);
    for name in ["N2", "Ar", "Ne", "CH4", "O2", "CH4_HCB", "N2_2S", "CO2_3S"] {
        let limit = own_limit(name);
        assert!(
            limit > DEFAULT_TIME_STEP,
            "{name}: {limit} limits the default step"
        );
    }
}

/// A species from a config `[[species]]` list gets a limit too: a smaller
/// one (stiffer, `U'' ∝ 1/σ²`) a smaller limit, a heavier one a larger.
#[test]
fn config_species_get_a_limit() {
    let cap = CapSettings::default();
    let limit_of = |mass: f32, sigma: f32| {
        let table = SpeciesTable::new(vec![Species::new("X", mass, sigma, 1.0, [1.0; 4])]);
        let limits = TimeStepLimits::new(&table, &Potential::lennard_jones(&table, cap, &[]), cap);
        let id = table.ids().next().unwrap();
        limits.pair(id, id)
    };
    let unit = limit_of(1.0, 1.0);
    assert!(unit.is_finite());
    assert!(limit_of(1.0, 0.5) < unit, "smaller σ");
    let heavy = limit_of(4.0, 1.0);
    assert!((heavy / unit - 2.0).abs() < 1e-3, "√m: {heavy} vs {unit}");
}

/// In a mixture the smallest pair limit applies, reported for the faster
/// species, and the limit follows the species present.
#[test]
fn a_mixture_takes_its_fastest_pair() {
    let table = SpeciesTable::builtin();
    let limits = builtin_limits();
    let (ar, h2) = (
        table.by_name("Ar").unwrap(),
        table.by_name("H2_2S").unwrap(),
    );
    let only_ar: SpeciesSet = [ar].into_iter().collect();
    let both: SpeciesSet = [ar, h2].into_iter().collect();
    assert_eq!(limits.limit(only_ar), Some((limits.pair(ar, ar), ar)));
    let (limit, by) = limits.limit(both).unwrap();
    assert_eq!(by, h2, "reported for H2");
    let smallest = limits
        .pair(h2, h2)
        .min(limits.pair(ar, h2))
        .min(limits.pair(ar, ar));
    assert_eq!(limit, smallest);
    assert_eq!(limits.limit(SpeciesSet::default()), None);
}

/// A null pair (a molecule's centre, `ε = 0`) has no stiffness.
#[test]
fn a_null_pair_has_no_stiffness() {
    let p = CappedPotential::new(
        LennardJones {
            sigma: 1.0,
            epsilon: 0.0,
        },
        CapSettings::default(),
    );
    assert_eq!(stiffness(&p, 1.0), 0.0);
}
