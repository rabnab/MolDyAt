// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::ball_physics::{BallPhysics, ForceResult, PhysicsConstants};
use crate::objects2d::ball_impl::pair_potential::{
    CapSettings, PairOverride, PairPotential, Potential,
};
use crate::objects2d::ball_impl::simd_forces::{KernelPoint, force_simd, kernel_points};
use crate::objects2d::ball_impl::{Ball2D, DEFAULT_SPECIES, SpeciesId, SpeciesTable};
use std::simd::f32x4;

/// The reference species (`σ = 1`).
const N2: SpeciesId = DEFAULT_SPECIES;

fn approx(a: f32, b: f32, eps: f32) {
    assert!(
        (a - b).abs() <= eps * (1.0 + a.abs().max(b.abs())),
        "values not approx equal: {} vs {} with eps {}",
        a,
        b,
        eps
    );
}

fn species(name: &str) -> SpeciesId {
    SpeciesTable::builtin().by_name(name).unwrap()
}

/// Ball 0 (`N2` at the origin) with every other ball as listed neighbour.
fn n2_with_neighbours(neighbours: &[(Vec2D, SpeciesId)]) -> Vec<Ball2D> {
    let mut balls = vec![Ball2D::new(0, Vec2D::new(0.0, 0.0), N2)];
    for (i, &(pos, species)) in neighbours.iter().enumerate() {
        balls.push(Ball2D::new(i + 1, pos, species));
        balls[0].hood.add_neighbour(i + 1);
    }
    balls
}

#[test]
fn test_simd_energy_conservation() {
    let physics = BallPhysics::default();
    let balls = n2_with_neighbours(&[(Vec2D::new(1.25, 0.0), N2)]);

    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    assert!(result.force.x.is_finite());
    assert!(result.force.y.is_finite());
    assert!(result.potential_energy.is_finite());
    // Centre distance 1.25 minus σ = 1: the contact discs are 0.25 apart.
    approx(result.min_dist, 0.25, 1e-6);
}

#[test]
fn test_overlapping_pair_is_repelled_and_reports_negative_gap() {
    let physics = BallPhysics::default();
    // Centre distance 0.5 < σ = 1: the contact discs overlap by 0.5.
    let balls = n2_with_neighbours(&[(Vec2D::new(0.5, 0.0), N2)]);

    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    approx(result.min_dist, -0.5, 1e-6);
    assert!(
        result.force.x < 0.0,
        "force should be repulsive (negative x)"
    );
}

#[test]
fn test_simd_potential_energy_correctness() {
    let physics = BallPhysics::default();

    // Test Case 1: Single neighbor at various centre distances
    for center_dist in [0.5, 0.8, 1.0, 1.122462, 2.0, 3.0] {
        let balls = n2_with_neighbours(&[(Vec2D::new(center_dist, 0.0), N2)]);
        let simd_result = force_simd(&physics, 0, &balls, &kernel_points(&balls));
        approx(
            simd_result.potential_energy,
            physics.potential(N2, N2, center_dist),
            1e-5,
        );
    }

    // Test Case 2: Multiple neighbors: the total is the sum of the pairs
    let positions = [
        Vec2D::new(1.25, 0.0),
        Vec2D::new(0.0, 1.5),
        Vec2D::new(1.75, 1.75),
    ];
    let balls = n2_with_neighbours(&positions.map(|p| (p, N2)));
    let simd_result = force_simd(&physics, 0, &balls, &kernel_points(&balls));
    let expected_total: f32 = positions
        .iter()
        .map(|p| physics.potential(N2, N2, p.norm()))
        .sum();
    approx(simd_result.potential_energy, expected_total, 1e-4);

    // Test Case 3: below r_cap the potential follows the quadratic continuation
    let r_cap = physics.get_force_cap_distance(N2, N2);
    let dist_capped = r_cap * 0.5;
    let balls = n2_with_neighbours(&[(Vec2D::new(dist_capped, 0.0), N2)]);
    let simd_result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    // Expected: U(r) = U(r_cap) - max_force·(r - r_cap) + ½·k·(r - r_cap)², shifted at the cutoff
    let shifted_pot_at_cap = physics.potential(N2, N2, r_cap);
    let max_force = physics.cap_settings().max_force;
    let stiffness = physics.cap_settings().core_stiffness;
    let compression = r_cap - dist_capped;
    let expected_core_pot =
        shifted_pot_at_cap + max_force * compression + 0.5 * stiffness * compression * compression;
    approx(simd_result.potential_energy, expected_core_pot, 1e-4);
}

// ── Virial tests (issue #112) ────────────────────────────────────

#[test]
fn test_virial_no_neighbours() {
    let physics = BallPhysics::default();
    let balls = n2_with_neighbours(&[]);
    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));
    assert_eq!(result.virial, 0.0, "Virial must be 0 with no neighbours");
}

#[test]
fn test_virial_single_neighbour_known_geometry() {
    let physics = BallPhysics::default();
    let center_dist: f32 = 1.5;
    let balls = n2_with_neighbours(&[(Vec2D::new(center_dist, 0.0), N2)]);

    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    // virial = Σ (r_ij · F_ij) = center_dist * lj_force (see derivation in issue #112)
    let expected_virial = center_dist * physics.force(N2, N2, center_dist);
    approx(result.virial, expected_virial, 1e-5);
}

#[test]
fn test_virial_force_capped_regime() {
    let physics = BallPhysics::default();
    let r_cap = physics.get_force_cap_distance(N2, N2);
    let max_force = physics.cap_settings().max_force;

    let center_dist = r_cap * 0.5;
    let balls = n2_with_neighbours(&[(Vec2D::new(center_dist, 0.0), N2)]);

    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    // In the capped regime the force rises linearly from its value at r_cap
    // (≈ max_force): F(r_cap) + k·(r_cap - r). virial = center_dist * core_force
    let force_at_cap = physics.force(N2, N2, r_cap);
    approx(force_at_cap, max_force, 1e-3);
    let core_force = force_at_cap + physics.cap_settings().core_stiffness * (r_cap - center_dist);
    approx(physics.force(N2, N2, center_dist), core_force, 1e-5);
    approx(result.virial, center_dist * core_force, 1e-4);
}

/// One SIMD chunk of four unlike pairs plus scalar remainders: every lane
/// must use its own pair's potential, and the chunk must agree with the
/// scalar functions pair by pair. Returns the kernel's result.
fn assert_mixed_chunk_matches_scalar_pairs(physics: &BallPhysics) -> ForceResult {
    let neighbours = [
        (Vec2D::new(1.3, 0.0), species("XS")),
        (Vec2D::new(0.0, 2.1), species("M")),
        (Vec2D::new(-3.0, 0.0), species("XL")),
        (Vec2D::new(0.0, -1.2), N2),
        (Vec2D::new(1.6, 1.6), species("L")), // scalar remainder
        (Vec2D::new(-1.1, -1.1), species("S")), // scalar remainder
    ];
    let balls = n2_with_neighbours(&neighbours);

    let result = force_simd(physics, 0, &balls, &kernel_points(&balls));

    let (mut force, mut potential, mut virial) = (Vec2D::new(0.0, 0.0), 0.0f32, 0.0f32);
    let mut min_gap = f32::INFINITY;
    let n2_contact = balls[0].contact_radius;
    for (i, &(pos, s)) in neighbours.iter().enumerate() {
        let r = pos.norm();
        let f = physics.force(N2, s, r);
        // Force on ball 0 points away from the neighbour when repulsive.
        force += pos.scaled(-f / r);
        potential += physics.potential(N2, s, r);
        virial += r * f;
        min_gap = min_gap.min(r - n2_contact - balls[i + 1].contact_radius);
    }
    approx(result.force.x, force.x, 1e-4);
    approx(result.force.y, force.y, 1e-4);
    approx(result.potential_energy, potential, 1e-4);
    approx(result.virial, virial, 1e-4);
    approx(result.min_dist, min_gap, 1e-5);
    result
}

#[test]
fn test_mixed_species_in_one_chunk_match_scalar_pairs() {
    assert_mixed_chunk_matches_scalar_pairs(&BallPhysics::default());
}

/// An overridden pair in the SIMD chunk (`N2`–`XS`, pushed into its core)
/// and one in the scalar remainder (`N2`–`S`): the kernel gathers the
/// overridden potentials like any other (#235).
#[test]
fn test_overridden_pairs_in_a_mixed_chunk_match_scalar_pairs() {
    let overrides = vec![
        PairOverride {
            a: species("XS"),
            b: N2,
            sigma: Some(1.6),
            epsilon: Some(2.0),
        },
        PairOverride {
            a: N2,
            b: species("S"),
            sigma: None,
            epsilon: Some(0.4),
        },
    ];
    let physics = BallPhysics::with_pair_overrides(
        PhysicsConstants::default(),
        SpeciesTable::builtin(),
        CapSettings::default(),
        overrides,
    );
    assert!(1.3 < physics.get_force_cap_distance(N2, species("XS")));
    let overridden = assert_mixed_chunk_matches_scalar_pairs(&physics);
    let mixed = assert_mixed_chunk_matches_scalar_pairs(&BallPhysics::default());
    assert_ne!(overridden.force, mixed.force, "the overrides must matter");
}

#[test]
fn test_listed_neighbours_beyond_cutoff_do_not_interact() {
    let physics = BallPhysics::default();
    // Six listed neighbours of mixed species (one SIMD chunk + two scalar
    // remainders), each inside its pair's list radius but beyond its cutoff.
    let kinds = ["N2", "XS", "S", "M", "L", "XL"].map(species);
    let neighbours: Vec<_> = kinds
        .iter()
        .enumerate()
        .map(|(k, &s)| {
            let center_dist = physics.cutoff(N2, s) + 0.1 * (k + 1) as f32;
            assert!(center_dist < physics.list_radius(N2, s));
            let angle = (k + 1) as f32;
            (
                Vec2D::new(center_dist * angle.cos(), center_dist * angle.sin()),
                s,
            )
        })
        .collect();
    let balls = n2_with_neighbours(&neighbours);

    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));
    assert_eq!(result.force, Vec2D::new(0.0, 0.0));
    assert_eq!(result.potential_energy, 0.0);
    assert_eq!(result.virial, 0.0);
}

#[test]
fn test_kernel_point_is_one_aligned_16_byte_record() {
    assert_eq!(std::mem::size_of::<KernelPoint>(), 16);
    assert_eq!(std::mem::align_of::<KernelPoint>(), 16);
}

#[test]
fn test_kernel_point_reads_pre_update_pos_species_and_contact_radius() {
    let m = species("M");
    let mut ball = Ball2D::new(0, Vec2D::new(1.0, 2.0), m);
    ball.pre_update_pos = Vec2D::new(4.0, 5.0);
    let point = KernelPoint::from(&ball);
    assert_eq!((point.x, point.y), (4.0, 5.0));
    assert_eq!(point.species as usize, m.index());
    assert_eq!(point.contact_radius, ball.contact_radius);
}

#[test]
fn test_simd_and_scalar_agree_in_the_core() {
    // The sloped core below r_cap (issue #240) must match between the f32x4
    // kernels and the scalar functions, down to full overlap.
    let physics = BallPhysics::default();
    let Potential::LennardJones(table) = physics.pair_potential();
    let pair = table.pair(N2, N2);
    let r_cap = pair.r_cap();
    let dists = [r_cap * 0.5, r_cap * 0.2, 0.05, 0.0];
    let force = pair.force_simd(f32x4::from(dists)).to_array();
    let potential = pair.potential_simd(f32x4::from(dists)).to_array();
    for (i, &d) in dists.iter().enumerate() {
        approx(force[i], pair.force(d), 1e-5);
        approx(potential[i], pair.potential(d), 1e-5);
    }
}
