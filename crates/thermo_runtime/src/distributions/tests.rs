// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Speed histogram and g(r) on sampled sets with known statistics (#343).

use super::*;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::DEFAULT_SPECIES;

/// A small deterministic generator (xorshift64*), uniform in `(0, 1)`.
struct Uniform(u64);

impl Uniform {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let bits = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40;
        (bits as f32 + 0.5) / (1u64 << 24) as f32
    }

    /// A standard normal number (Box–Muller).
    fn normal(&mut self) -> f32 {
        let (u, v) = (self.next(), self.next());
        (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
    }
}

fn ball(species: SpeciesId, mass: f32, position: Vec2D, velocity: Vec2D) -> BallRenderData {
    BallRenderData {
        id: 0,
        species,
        position,
        velocity,
        mass,
        color: [1.0; 4],
        contact_radius: 0.5,
        display_radius: 0.5,
        equilibrium_radius: 0.56,
        cutoff_radius: 2.0,
    }
}

/// `n` balls of `mass` with Maxwell–Boltzmann velocities at `temperature`.
fn maxwell_gas(n: usize, mass: f32, temperature: f32, rng: &mut Uniform) -> Vec<BallRenderData> {
    let sigma = (temperature / mass).sqrt();
    (0..n)
        .map(|_| {
            let v = Vec2D::new(sigma * rng.normal(), sigma * rng.normal());
            ball(DEFAULT_SPECIES, mass, Vec2D::zero(), v)
        })
        .collect()
}

/// Every bin within 5 standard deviations of the Maxwell–Boltzmann count
/// (Poisson noise), and the totals equal.
fn assert_matches_maxwell(histogram: &SpeedHistogram, n: usize) {
    // The curve's tail past the fastest ball holds about one ball.
    let expected_total: f32 = histogram.expected.iter().sum();
    assert!(
        (0.0..5.0).contains(&(n as f32 - expected_total)),
        "{expected_total}"
    );
    assert_eq!(histogram.counts.iter().sum::<u32>() as usize, n);
    for (i, (&count, &expected)) in histogram.counts.iter().zip(&histogram.expected).enumerate() {
        let tolerance = 5.0 * expected.max(1.0).sqrt() + 2.0;
        assert!(
            (count as f32 - expected).abs() <= tolerance,
            "bin {i}: {count} balls, {expected:.1} expected"
        );
    }
}

#[test]
fn a_sampled_maxwell_boltzmann_gas_matches_the_analytic_curve() {
    let mut rng = Uniform(343);
    let gas = maxwell_gas(20_000, 1.0, 1.5, &mut rng);
    let histogram = speed_histogram(&gas, SPEED_BINS).expect("a moving gas");
    assert!(
        (histogram.temperature - 1.5).abs() < 0.05,
        "{}",
        histogram.temperature
    );
    assert_matches_maxwell(&histogram, gas.len());
}

#[test]
fn a_mixture_matches_the_sum_of_its_species_curves() {
    let mut rng = Uniform(7);
    let mut gas = maxwell_gas(10_000, 1.0, 1.0, &mut rng);
    gas.extend(maxwell_gas(10_000, 4.0, 1.0, &mut rng));
    let histogram = speed_histogram(&gas, SPEED_BINS).expect("a moving gas");
    assert_matches_maxwell(&histogram, gas.len());
}

#[test]
fn speeds_are_taken_in_the_centre_of_mass_frame() {
    let mut rng = Uniform(11);
    let mut gas = maxwell_gas(2_000, 1.0, 1.0, &mut rng);
    let at_rest = speed_histogram(&gas, SPEED_BINS).unwrap();
    for ball in &mut gas {
        ball.velocity = Vec2D::new(ball.velocity.x + 50.0, ball.velocity.y);
    }
    let drifting = speed_histogram(&gas, SPEED_BINS).unwrap();
    assert_eq!(drifting.counts, at_rest.counts);
    assert!((drifting.temperature - at_rest.temperature).abs() < 1e-3);
}

#[test]
fn no_histogram_without_motion() {
    let resting = vec![ball(DEFAULT_SPECIES, 1.0, Vec2D::zero(), Vec2D::zero()); 3];
    assert_eq!(speed_histogram(&resting, SPEED_BINS), None);
    assert_eq!(speed_histogram(&resting[..1], SPEED_BINS), None);
}

/// `n` balls at uniformly random positions in `world`.
fn ideal_gas(n: usize, world: &Rect, species: SpeciesId, rng: &mut Uniform) -> Vec<BallRenderData> {
    (0..n)
        .map(|_| {
            let position = Vec2D::new(
                world.x + world.w * rng.next(),
                world.y + world.h * rng.next(),
            );
            ball(species, 1.0, position, Vec2D::zero())
        })
        .collect()
}

#[test]
fn g_of_an_ideal_gas_is_one() {
    let world = Rect::new(0.0, 0.0, 100.0, 100.0);
    let mut rng = Uniform(290);
    let gas = ideal_gas(8_000, &world, DEFAULT_SPECIES, &mut rng);
    let rdf = radial_distribution(&gas, &world, RDF_MAX_DISTANCE, RDF_BINS);
    assert_eq!(rdf.pairs.len(), 1);
    let pair = &rdf.pairs[0];
    assert!(pair.reference_balls > 6_000, "{}", pair.reference_balls);
    // Past the first σ every shell holds hundreds of pairs: g within noise of 1.
    let far: Vec<f32> = pair
        .g
        .iter()
        .enumerate()
        .filter(|&(i, _)| rdf.bin_centre(i) > 1.0)
        .map(|(_, &g)| g)
        .collect();
    let mean = far.iter().sum::<f32>() / far.len() as f32;
    assert!((mean - 1.0).abs() < 0.02, "mean g {mean}");
    assert!(far.iter().all(|g| (g - 1.0).abs() < 0.2), "{far:?}");
}

#[test]
fn a_mixture_has_one_g_per_species_pair() {
    let world = Rect::new(-50.0, 50.0, 100.0, -100.0);
    let mut rng = Uniform(3);
    let other = SpeciesId::new(6);
    let mut gas = ideal_gas(3_000, &world, DEFAULT_SPECIES, &mut rng);
    gas.extend(ideal_gas(3_000, &world, other, &mut rng));
    let rdf = radial_distribution(&gas, &world, RDF_MAX_DISTANCE, RDF_BINS);
    let species: Vec<_> = rdf.pairs.iter().map(|p| p.species).collect();
    assert_eq!(
        species,
        [
            (DEFAULT_SPECIES, DEFAULT_SPECIES),
            (DEFAULT_SPECIES, other),
            (other, other)
        ]
    );
    for pair in &rdf.pairs {
        let mean = pair.g[10..].iter().sum::<f32>() / (RDF_BINS - 10) as f32;
        assert!((mean - 1.0).abs() < 0.05, "{:?}: {mean}", pair.species);
    }
}

#[test]
fn no_bins_give_no_pairs() {
    let world = Rect::new(0.0, 0.0, 50.0, 50.0);
    let gas = ideal_gas(20, &world, DEFAULT_SPECIES, &mut Uniform(2));
    assert!(
        radial_distribution(&gas, &world, RDF_MAX_DISTANCE, 0)
            .pairs
            .is_empty()
    );
    assert!(
        radial_distribution(&gas, &world, 0.0, RDF_BINS)
            .pairs
            .is_empty()
    );
}

#[test]
fn a_world_too_small_for_the_shells_has_no_g() {
    let world = Rect::new(0.0, 0.0, 8.0, 8.0);
    let gas = ideal_gas(20, &world, DEFAULT_SPECIES, &mut Uniform(1));
    let rdf = radial_distribution(&gas, &world, RDF_MAX_DISTANCE, RDF_BINS);
    assert_eq!(rdf.pairs[0].reference_balls, 0);
    assert!(rdf.pairs[0].g.iter().all(|g| g.is_nan()));
}
