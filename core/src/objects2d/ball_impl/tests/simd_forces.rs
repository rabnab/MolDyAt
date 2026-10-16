// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::ball_impl::ball_physics::BallPhysics;
use crate::objects2d::ball_impl::simd_forces::{KernelPoint, force_simd, kernel_points};
use std::simd::f32x4;

fn approx(a: f32, b: f32, eps: f32) {
    assert!(
        (a - b).abs() <= eps * (1.0 + a.abs().max(b.abs())),
        "values not approx equal: {} vs {} with eps {}",
        a,
        b,
        eps
    );
}

#[test]
fn test_simd_energy_conservation() {
    let physics = BallPhysics::default();

    // Create two balls
    let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
    let ball_b = Ball2D::new(1, Vec2D::new(2.5, 0.0), (0, 1.0));

    ball_a.hood.add_neighbour(1);
    let balls = vec![ball_a, ball_b];

    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    // Force should be finite
    assert!(result.force.x.is_finite());
    assert!(result.force.y.is_finite());

    // Potential energy should be finite
    assert!(result.potential_energy.is_finite());

    // Min distance should be positive
    assert!(result.min_dist > 0.0);
}

#[test]
fn test_simd_vs_scalar_negative_edge_dist() {
    let physics = BallPhysics::default();

    // Create two balls that are overlapping (negative edge distance)
    let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
    // Position ball_b very close - closer than the sum of radii
    // Sum of radii = 2.0, so distance < 2.0 means negative edge_dist
    let ball_b = Ball2D::new(1, Vec2D::new(1.5, 0.0), (0, 1.0));

    // Set up neighbor list
    ball_a.hood.add_neighbour(1);

    let balls = vec![ball_a, ball_b];

    // Calculate with SIMD version
    let simd_result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    // When balls overlap (edge_dist < 0), both should set min_dist to 0.0
    assert_eq!(
        simd_result.min_dist, 0.0,
        "SIMD min_dist should be 0.0 for overlapping balls"
    );

    // Both scalar and SIMD now use max_force when edge_dist <= 0
    // They should produce the same repulsive forces in the same direction
    assert!(
        simd_result.force.x < 0.0,
        "SIMD force should be repulsive (negative x)"
    );
}

#[test]
fn test_simd_potential_energy_correctness() {
    let physics = BallPhysics::default();

    // Test Case 1: Single neighbor at various distances
    // We'll verify that the SIMD potential matches the scalar physics.potential() method
    let test_distances = vec![
        0.5,      // Very close (force-capped region)
        0.8,      // Still in capped region
        1.0,      // Near sigma
        1.122462, // Equilibrium distance (2^(1/6) * sigma)
        2.0,      // Beyond equilibrium
        3.0,      // Far away
    ];

    for &center_dist in &test_distances {
        let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
        let ball_b = Ball2D::new(1, Vec2D::new(center_dist, 0.0), (0, 1.0));

        ball_a.hood.add_neighbour(1);
        let balls = vec![ball_a, ball_b];

        // Calculate edge distance
        let edge_dist = center_dist - 2.0; // Both radii = 1.0

        // Get SIMD result
        let simd_result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

        // Calculate expected potential using scalar method
        let expected_potential = physics.potential(edge_dist);

        // Verify SIMD potential matches scalar potential
        approx(simd_result.potential_energy, expected_potential, 1e-5);
    }

    // Test Case 2: Multiple neighbors
    // Verify that total potential is the sum of individual pair potentials
    let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
    let ball_b = Ball2D::new(1, Vec2D::new(2.5, 0.0), (0, 1.0));
    let ball_c = Ball2D::new(2, Vec2D::new(0.0, 3.0), (0, 1.0));
    let ball_d = Ball2D::new(3, Vec2D::new(3.5, 3.5), (0, 1.0));

    ball_a.hood.add_neighbour(1);
    ball_a.hood.add_neighbour(2);
    ball_a.hood.add_neighbour(3);

    let balls = vec![ball_a, ball_b, ball_c, ball_d];

    let simd_result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    // Calculate expected total potential manually
    let edge_dist_ab = 2.5 - 2.0; // 0.5
    let edge_dist_ac = 3.0 - 2.0; // 1.0
    let edge_dist_ad = ((3.5 * 3.5 + 3.5 * 3.5) as f32).sqrt() - 2.0; // ~2.95

    let expected_total = physics.potential(edge_dist_ab)
        + physics.potential(edge_dist_ac)
        + physics.potential(edge_dist_ad);

    approx(simd_result.potential_energy, expected_total, 1e-4);

    // Test Case 3: Verify force-capped region uses the quadratic potential
    // When edge_dist < r_cap, potential follows the quadratic continuation
    let r_cap = physics.get_force_cap_distance();
    let edge_dist_capped = r_cap * 0.5; // Well below r_cap

    let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
    let ball_b = Ball2D::new(1, Vec2D::new(edge_dist_capped + 2.0, 0.0), (0, 1.0));

    ball_a.hood.add_neighbour(1);
    let balls = vec![ball_a, ball_b];

    let simd_result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    // Expected: U(r) = U(r_cap) - max_force·(r - r_cap) + ½·k·(r - r_cap)², shifted at the cutoff
    let shifted_pot_at_cap = physics.potential(r_cap);
    let max_force = physics.constants.max_force;
    let stiffness = physics.constants.core_stiffness;
    let compression = r_cap - edge_dist_capped;
    let expected_core_pot =
        shifted_pot_at_cap + max_force * compression + 0.5 * stiffness * compression * compression;

    approx(simd_result.potential_energy, expected_core_pot, 1e-4);
}

// ── Virial tests (issue #112) ────────────────────────────────────

#[test]
fn test_virial_no_neighbours() {
    let physics = BallPhysics::default();
    let ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
    let balls = vec![ball_a];
    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));
    assert_eq!(result.virial, 0.0, "Virial must be 0 with no neighbours");
}

#[test]
fn test_virial_single_neighbour_known_geometry() {
    let physics = BallPhysics::default();

    // Ball A at origin, Ball B at (2.5, 0). Both have radius 1.0.
    // center_dist = 2.5, edge_dist = 0.5
    let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
    let ball_b = Ball2D::new(1, Vec2D::new(2.5, 0.0), (0, 1.0));
    ball_a.hood.add_neighbour(1);
    let balls = vec![ball_a, ball_b];

    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    // virial = Σ (r_ij · F_ij) = center_dist * lj_force (see derivation in issue #112)
    let center_dist: f32 = 2.5;
    let edge_dist: f32 = 0.5;
    let lj_force = physics.force(edge_dist);
    let expected_virial = center_dist * lj_force;

    approx(result.virial, expected_virial, 1e-5);
}

#[test]
fn test_virial_force_capped_regime() {
    let physics = BallPhysics::default();
    let r_cap = physics.get_force_cap_distance();
    let max_force = physics.constants.max_force;

    // Place neighbour so edge_dist is well below r_cap
    let edge_dist_capped = r_cap * 0.5;
    let center_dist = edge_dist_capped + 2.0; // both radii = 1.0

    let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
    let ball_b = Ball2D::new(1, Vec2D::new(center_dist, 0.0), (0, 1.0));
    ball_a.hood.add_neighbour(1);
    let balls = vec![ball_a, ball_b];

    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    // In the capped regime the force rises linearly from its value at r_cap
    // (≈ max_force): F(r_cap) + k·(r_cap - r). virial = center_dist * core_force
    let force_at_cap = physics.force(r_cap);
    approx(force_at_cap, max_force, 1e-3);
    let core_force = force_at_cap + physics.constants.core_stiffness * (r_cap - edge_dist_capped);
    approx(physics.force(edge_dist_capped), core_force, 1e-5);
    let expected_virial = center_dist * core_force;

    approx(result.virial, expected_virial, 1e-4);
}

#[test]
fn test_virial_simd_and_scalar_paths_agree() {
    let physics = BallPhysics::default();

    // 5 neighbours: 4 processed by SIMD chunk + 1 by scalar remainder.
    // Place them at distinct distances so each contributes a different virial.
    let positions = [
        Vec2D::new(2.5, 0.0),
        Vec2D::new(0.0, 3.0),
        Vec2D::new(-3.5, 0.0),
        Vec2D::new(0.0, -4.0),
        Vec2D::new(3.0, 3.0), // 5th → scalar remainder
    ];

    let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
    for i in 0..positions.len() {
        ball_a.hood.add_neighbour(i + 1);
    }
    let mut balls = vec![ball_a];
    for (i, &pos) in positions.iter().enumerate() {
        let b = Ball2D::new(i + 1, pos, (0, 1.0));
        balls.push(b);
    }

    let result = force_simd(&physics, 0, &balls, &kernel_points(&balls));

    // Compute expected virial by scalar method for each pair
    let ball_pos = Vec2D::new(0.0, 0.0);
    let ball_radius = 1.0f32;
    let mut expected_virial = 0.0f32;
    for &pos in &positions {
        let dx = ball_pos.x - pos.x;
        let dy = ball_pos.y - pos.y;
        let center_dist = (dx * dx + dy * dy).sqrt();
        let edge_dist = center_dist - ball_radius - 1.0; // neighbour radius = 1.0
        let lj_force = physics.force(edge_dist);
        // virial_pair = center_dist * lj_force (derived from dx*fx + dy*fy)
        expected_virial += center_dist * lj_force;
    }

    approx(result.virial, expected_virial, 1e-4);
}

#[test]
fn test_listed_neighbours_beyond_cutoff_do_not_interact() {
    let physics = BallPhysics::default();
    let cutoff = physics.get_cutoff_distance();
    let radius = 0.5;
    // Five listed neighbours (one SIMD chunk + one scalar remainder), all
    // inside the neighbour-list radius but beyond the LJ cutoff.
    let mut balls = vec![Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, radius))];
    for k in 1..=5 {
        let center_dist = 2.0 * radius + cutoff + 0.1 * k as f32;
        let angle = k as f32;
        let pos = Vec2D::new(center_dist * angle.cos(), center_dist * angle.sin());
        balls.push(Ball2D::new(k, pos, (0, radius)));
        balls[0].hood.add_neighbour(k);
    }

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
fn test_kernel_point_reads_pre_update_pos_and_radius() {
    let mut ball = Ball2D::new(0, Vec2D::new(1.0, 2.0), (0, 3.0));
    ball.pre_update_pos = Vec2D::new(4.0, 5.0);
    let point = KernelPoint::from(&ball);
    assert_eq!((point.x, point.y, point.radius), (4.0, 5.0, 3.0));
}

#[test]
fn test_simd_and_scalar_agree_in_the_core() {
    // The sloped core below r_cap (issue #240) must match between the f32x4
    // kernels and the scalar functions, including overlaps.
    let physics = BallPhysics::default();
    let r_cap = physics.get_force_cap_distance();
    let dists = [r_cap * 0.5, 0.0, -0.4, -1.18];
    let force = physics.force_simd(f32x4::from(dists)).to_array();
    let potential = physics.potential_simd(f32x4::from(dists)).to_array();
    for (i, &d) in dists.iter().enumerate() {
        approx(force[i], physics.force(d), 1e-5);
        approx(potential[i], physics.potential(d), 1e-5);
    }
}
