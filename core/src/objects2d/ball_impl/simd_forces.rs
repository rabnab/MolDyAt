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

/// Calculate Lennard-Jones forces for multiple ball pairs using SIMD vectorization.
/// Processes neighbors in chunks of 4 using CPU SIMD instructions (SSE2/AVX2/NEON).
/// Applies force capping at r < r_cap with linear potential extrapolation for energy conservation.
/// Conditionally collects per-neighbor force magnitudes only when visualization is enabled.
///
/// # Arguments
/// * `physics` - Physics engine with LJ parameters and force cap settings
/// * `ball_idx` - Index of ball to calculate forces for
/// * `balls` - Slice of all balls (read-only access)
///
/// # Returns
/// ForceResult containing total force vector, potential energy, minimum distance, and optional neighbor forces
pub fn force_simd(physics: &BallPhysics, ball_idx: usize, balls: &[Ball2D]) -> ForceResult {
    let mut force = Vec2D::new(0., 0.);
    let mut min_dist = f32::NAN;
    let mut total_pot = 0.;
    let mut virial = 0.0f32;

    // Optimization: Only collect neighbor forces if visualization is enabled for this ball
    // This avoids expensive allocations and vector pushes in the hot path
    let collect_neighbor_forces = balls[ball_idx].verbose.lines();
    let mut neighbor_forces = if collect_neighbor_forces {
        Vec::with_capacity(balls[ball_idx].hood.get_number_of_neighbors())
    } else {
        Vec::new()
    };

    let ball_pos = balls[ball_idx].get_position();
    let ball_radius = balls[ball_idx].radius;
    let neighbour_indices = balls[ball_idx].hood.get_neighbour_indices();

    let lj_sig = physics.constants.lj_sig;
    let lj_eps = physics.constants.lj_eps;
    let r_cap = physics.get_force_cap_distance();

    // Process neighbors in chunks of 4 for SIMD
    let (chunks, unchunked) = neighbour_indices.as_chunks::<4>();
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

        // Virial: Σ (r_ij · F_ij).
        // Since fx = dx_vec * force_to_dist and fy = dy_vec * force_to_dist,
        // we have r · F = (dx^2 + dy^2) * force_to_dist = center_dist_sq * force_to_dist.
        virial += (center_dist_sq * force_to_dist).reduce_sum();

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
        if collect_neighbor_forces {
            let lj_force_array: [f32; 4] = lj_force.abs().to_array();
            for (i, &neighbour_idx) in chunk.iter().enumerate() {
                neighbor_forces.push((neighbour_idx, lj_force_array[i]));
            }
        }
    }

    // Process remaining neighbors with scalar code
    for &neighbour_idx in unchunked {
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

            // Virial: r_ij · F_ij = dx * fx + dy * fy
            virial += dist_vec.x * fx + dist_vec.y * fy;

            force.x += fx;
            force.y += fy;

            total_pot += physics.potential(edge_dist);

            // Store per-neighbor force magnitude for visualization
            if collect_neighbor_forces {
                neighbor_forces.push((neighbour_idx, lj_force.abs()));
            }
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
        virial,
    }
}
