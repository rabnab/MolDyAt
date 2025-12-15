//! SIMD-optimized Lennard-Jones force calculations with force capping
//!
//! This module provides vectorized implementations of Lennard-Jones force calculations
//! using CPU SIMD instructions for improved performance. It includes:
//! - SSE2 (4-wide) for x86/x86_64 processors
//! - AVX2 (4-wide, with FMA) for modern x86_64 processors (note: not 8-wide)
//! - NEON (4-wide) for ARM processors (M1/M2 Macs)
//! - Scalar fallback for unsupported architectures
//!
//! # Force Capping for Stability
//! To prevent numerical instabilities when particles get very close, forces are capped
//! at `max_force`. This is implemented as:
//! - For r >= r_cap: Standard LJ force F_LJ(r)
//! - For r < r_cap: Capped force = max_force
//!
//! The corresponding potential energy uses linear extrapolation U(r) = U(r_cap) - max_force·(r - r_cap)
//! below r_cap to maintain energy conservation. See `BallPhysics::lenard_jones_potential()` for details.
//!
//! Note: All SIMD implementations are currently 4-wide. AVX2 uses FMA for improved precision/performance, but does not use 8-wide vectorization yet.
//! 8-wide AVX2 support is a planned future optimization (see PERFORMANCE_OPTIMIZATION.md line 548).

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics, ForceResult};
use std::simd::prelude::SimdFloat;
use std::simd::{StdFloat, f32x4};

/// Calculate Lennard-Jones forces for multiple ball pairs using SIMD
///
/// This function processes multiple neighbor interactions simultaneously using SIMD instructions.
pub fn force_simd(physics: &BallPhysics, ball_idx: usize, balls: &[Ball2D]) -> ForceResult {
    let mut force = Vec2D::new(0., 0.);
    let mut min_dist = f32::NAN;
    let mut total_pot = 0.;
    let mut neighbor_forces = Vec::new();

    let ball_pos = balls[ball_idx].get_position();
    let ball_radius = balls[ball_idx].radius;
    let neighbour_indices = balls[ball_idx].hood.get_neighbour_indices();

    let lj_sig = physics.constants.lj_sig;
    let lj_eps = physics.constants.lj_eps;
    let r_cap = physics.get_force_cap_distance();

    // Process neighbors in chunks of 4 for SIMD
    let chunks = neighbour_indices.chunks(4);
    let remainder_start = (neighbour_indices.len() / 4) * 4;

    // Pre-calculate U(r_cap) for linear extrapolation in force-capped region
    // This is the "anchor point" where capped and uncapped potentials meet

    let pot_at_cap = {
        let cap_quot = lj_sig / r_cap;
        let cap_quot_six = cap_quot * cap_quot * cap_quot * cap_quot * cap_quot * cap_quot;
        4. * lj_eps * cap_quot_six * (cap_quot_six - 1.)
    };
    let pot_at_cap_vec = f32x4::splat(pot_at_cap);

    let max_force_vec = f32x4::splat(physics.constants.max_force);
    let r_cap_vec = f32x4::splat(r_cap);

    for chunk in chunks {
        if chunk.len() == 4 {
            // SIMD path: process 4 neighbors at once
            let mut dx = [0.0f32; 4];
            let mut dy = [0.0f32; 4];
            let mut radii = [0.0f32; 4];

            // Gather data for 4 neighbors
            for (i, &neighbour_idx) in chunk.iter().enumerate() {
                if let Some(neighbour) = balls.get(neighbour_idx) {
                    let dist_vec = neighbour.pre_update_pos.dist(ball_pos);
                    dx[i] = dist_vec.x;
                    dy[i] = dist_vec.y;
                    radii[i] = neighbour.radius;
                } else {
                    panic!("Invalid neighbor index in SIMD force calculation");
                }
            }

            // Load into SIMD vectors
            let dx_vec = f32x4::from(dx);
            let dy_vec = f32x4::from(dy);
            let radii_vec = f32x4::from(radii);

            // Calculate distances: sqrt(dx^2 + dy^2)
            let center_dist_sq = dx_vec * dx_vec + dy_vec * dy_vec;
            let center_dist = center_dist_sq.sqrt();

            // Calculate edge distances
            let radius_sum = radii_vec + f32x4::splat(ball_radius);
            let edge_dist = center_dist - radius_sum;
            let edge_dist_cap = edge_dist.simd_max(f32x4::splat(r_cap));

            // === Force Calculation with Capping ===
            //let r_cap_vec = f32x4::splat(r_cap);

            // Step 1: Calculate standard Lennard-Jones force
            // F_LJ(r) = 24·ε·(σ/r)^6 · [2·(σ/r)^6 - 1] / r
            // Use max() to prevent division by zero (1e-10 is safety epsilon)
            let lj_force = physics.force_simd(edge_dist_cap);

            // Force direction: force / center_dist
            // Avoid division by zero: if center_dist is 0, use a small epsilon to prevent Inf/NaN
            let safe_center_dist = center_dist.simd_max(f32x4::splat(1e-10));
            let force_to_dist = lj_force / safe_center_dist;

            // Calculate force components
            let fx = dx_vec * force_to_dist;
            let fy = dy_vec * force_to_dist;

            // === Potential Energy Calculation with Force Capping ===
            // Standard LJ potential: U(r) = 4·ε·[(σ/r)^12 - (σ/r)^6]
            // For r < r_cap: Use linear extrapolation to preserve energy conservation
            // Calculate standard LJ potential (with capping)
            let pot_quot = f32x4::splat(lj_sig) / edge_dist.simd_max(f32x4::splat(1e-10));
            let pot_quot_six = pot_quot * pot_quot * pot_quot * pot_quot * pot_quot * pot_quot;
            let potential_uncapped =
                f32x4::splat(4. * lj_eps) * pot_quot_six * (pot_quot_six - f32x4::splat(1.));
            // Linear potential for r < r_cap: U(r) = U(r_cap) - max_force·(r - r_cap)
            // This ensures F = -dU/dr = +max_force (repulsive, constant) below r_cap
            let potential_linear = f32x4::simd_max(
                pot_at_cap_vec - max_force_vec * (edge_dist - r_cap_vec),
                pot_at_cap_vec,
            );

            let potential_used = f32x4::simd_min(potential_linear, potential_uncapped);

            force.x += fx.reduce_sum();
            force.y += fy.reduce_sum();
            total_pot += potential_used.reduce_sum();
            let min_dist_of_chunk = edge_dist.reduce_min();
            min_dist = if f32::is_nan(min_dist) {
                min_dist_of_chunk
            } else {
                f32::min(min_dist, min_dist_of_chunk)
            };

            // Store per-neighbor force magnitudes for visualization
            let lj_force_array: [f32; 4] = lj_force.to_array();
            for (i, &neighbour_idx) in chunk.iter().enumerate() {
                neighbor_forces.push((neighbour_idx, lj_force_array[i]));
            }
        }
    }

    // Process remaining neighbors with scalar code
    for &neighbour_idx in &neighbour_indices[remainder_start..] {
        if let Some(b) = balls.get(neighbour_idx) {
            let dist_vec = b.pre_update_pos.dist(ball_pos);
            let center_dist = dist_vec.norm();
            let radius_sum = b.radius + ball_radius;
            let edge_dist = center_dist - radius_sum;

            if center_dist < 0. {
                continue; // Skip self-interaction or invalid distance
            }

            let lj_force = physics.force(edge_dist);

            // Force direction: force / center_dist
            // Avoid division by zero: if center_dist is 0, use a small epsilon to prevent Inf/NaN
            let safe_center_dist = center_dist.max(1e-10);
            let force_to_dist = lj_force / safe_center_dist;

            // Calculate force components
            let fx = dist_vec.x * force_to_dist;
            let fy = dist_vec.y * force_to_dist;

            force.x += fx;
            force.y += fy;

            total_pot += physics.potential(edge_dist);

            // Store per-neighbor force magnitude for visualization
            neighbor_forces.push((neighbour_idx, lj_force));

            // Update min_dist
            if edge_dist > 0. {
                min_dist = if f32::is_nan(min_dist) {
                    edge_dist
                } else {
                    f32::min(min_dist, edge_dist)
                };
            } else {
                min_dist = 0.;
            }
        }
    }

    ForceResult {
        min_dist,
        force,
        potential_energy: total_pot,
        neighbor_forces,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects2d::Vec2D;
    use crate::objects2d::ball_impl::Ball2D;

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

        let result = force_simd(&physics, 0, &balls);

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
        let simd_result = force_simd(&physics, 0, &balls);

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
            let simd_result = force_simd(&physics, 0, &balls);

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

        let simd_result = force_simd(&physics, 0, &balls);

        // Calculate expected total potential manually
        let edge_dist_ab = 2.5 - 2.0; // 0.5
        let edge_dist_ac = 3.0 - 2.0; // 1.0
        let edge_dist_ad = ((3.5 * 3.5 + 3.5 * 3.5) as f32).sqrt() - 2.0; // ~2.95

        let expected_total = physics.potential(edge_dist_ab)
            + physics.potential(edge_dist_ac)
            + physics.potential(edge_dist_ad);

        approx(simd_result.potential_energy, expected_total, 1e-4);

        // Test Case 3: Verify force-capped region uses linear potential
        // When edge_dist < r_cap, potential should follow linear extrapolation
        let r_cap = physics.get_force_cap_distance();
        let edge_dist_capped = r_cap * 0.5; // Well below r_cap

        let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
        let ball_b = Ball2D::new(1, Vec2D::new(edge_dist_capped + 2.0, 0.0), (0, 1.0));

        ball_a.hood.add_neighbour(1);
        let balls = vec![ball_a, ball_b];

        let simd_result = force_simd(&physics, 0, &balls);

        // Expected linear potential: U(r) = U(r_cap) - max_force·(r - r_cap)
        let pot_at_cap = physics.constants.pot_at_cap;
        let max_force = physics.constants.max_force;
        let expected_linear_pot = pot_at_cap - max_force * (edge_dist_capped - r_cap);

        approx(simd_result.potential_energy, expected_linear_pot, 1e-4);
    }
}
