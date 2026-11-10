// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The per-ball kinetics helper: degrees of freedom, energy, scaling,
//! stopping and sampling for balls with and without orientation (#429).

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::ball_impl::ball_factory::BallFactory;
use crate::objects2d::ball_impl::species::SpeciesTable;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

fn ball(name: &str, v: Vec2D, omega: f32) -> Ball2D {
    let species = SpeciesTable::builtin().by_name(name).unwrap();
    let mut ball = BallFactory::new().create_with_species_and_velocity(Vec2D::zero(), species, v);
    if ball.inertia > 0.0 {
        ball.omega = omega;
    }
    ball
}

#[test]
fn dof_counts_rotation_only_with_an_orientation() {
    assert_eq!(ball("Ar", Vec2D::zero(), 0.0).dof(), 2);
    assert_eq!(ball("N2_2S", Vec2D::zero(), 0.0).dof(), 3);
}

#[test]
fn kinetic_energy_adds_translation_and_rotation() {
    let atom = ball("Ar", Vec2D::new(1.0, 2.0), 5.0);
    assert_eq!(atom.kinetic_energy(), f64::from(0.5 * atom.mass * 5.0));

    let molecule = ball("N2_2S", Vec2D::new(1.0, 2.0), 5.0);
    let expected = 0.5 * f64::from(molecule.mass) * 5.0 + 0.5 * f64::from(molecule.inertia) * 25.0;
    assert!((molecule.kinetic_energy() - expected).abs() < 1e-5 * expected);
}

#[test]
fn scale_about_keeps_the_centre_and_scales_the_spin() {
    let centre = Vec2D::new(1.0, 1.0);
    let mut molecule = ball("N2_2S", Vec2D::new(3.0, 1.0), 4.0);
    molecule.scale_about(centre, 0.5);
    assert_eq!(molecule.v, Vec2D::new(2.0, 1.0));
    assert_eq!(molecule.omega, 2.0);

    let mut atom = ball("Ar", Vec2D::new(3.0, 1.0), 0.0);
    atom.scale_about(centre, 0.5);
    assert_eq!((atom.v, atom.omega), (Vec2D::new(2.0, 1.0), 0.0));
}

#[test]
fn stop_clears_translation_and_rotation() {
    let mut molecule = ball("N2_2S", Vec2D::new(3.0, 1.0), 4.0);
    molecule.stop();
    assert_eq!(molecule.kinetic_energy(), 0.0);
    assert_eq!((molecule.v, molecule.omega), (Vec2D::zero(), 0.0));
}

#[test]
fn sample_gives_each_degree_of_freedom_the_same_temperature() {
    let temperature = 2.0_f32;
    let mut rng = StdRng::seed_from_u64(7);
    let mut template = ball("N2_2S", Vec2D::zero(), 0.0);
    let (mut twice_energy, n) = (0.0_f64, 20_000);
    for _ in 0..n {
        template.sample(temperature, &mut rng);
        twice_energy += 2.0 * template.kinetic_energy();
    }
    // Equipartition: ⟨E⟩ = dof · T / 2.
    let mean_temperature = twice_energy / f64::from(n) / template.dof() as f64;
    assert!((mean_temperature - f64::from(temperature)).abs() < 0.05);
}

#[test]
fn sample_spin_draws_nothing_without_orientation_or_temperature() {
    let mut rng = StdRng::seed_from_u64(3);
    let mut untouched = StdRng::seed_from_u64(3);
    ball("Ar", Vec2D::zero(), 0.0).sample_spin(1.0, &mut rng);
    ball("N2_2S", Vec2D::zero(), 0.0).sample_spin(0.0, &mut rng);
    assert_eq!(rng.random::<u64>(), untouched.random::<u64>());
}
