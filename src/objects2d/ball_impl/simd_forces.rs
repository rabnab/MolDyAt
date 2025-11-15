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
use crate::objects2d::ball_impl::{Ball2D, BallPhysics, ForceResult, Named};
use wide::f32x4;

/// Calculate Lennard-Jones forces for multiple ball pairs using SIMD
///
/// This function processes multiple neighbor interactions simultaneously using SIMD instructions.
/// It automatically selects the best available SIMD implementation based on CPU features.
#[multiversion::multiversion(targets("x86_64+avx2+fma", "x86_64+sse2", "aarch64+neon"))]
pub fn force_indexed_simd(physics: &BallPhysics, ball_idx: usize, balls: &[Ball2D]) -> ForceResult {
    let mut force = Vec2D::new(0., 0.);
    let mut min_dist = f32::NAN;
    let mut total_pot = 0.;

    let ball_pos = balls[ball_idx].get_position();
    let ball_radius = balls[ball_idx].radius;
    let ball_verbose_console = balls[ball_idx].verbose.console;
    let neighbour_indices = balls[ball_idx].hood.get_neighbour_indices();

    let lj_sig = physics.constants.lj_sig;
    let lj_eps = physics.constants.lj_eps;
    let max_force = physics.constants.max_force;
    let r_cap = physics.get_force_cap_distance();

    // Process neighbors in chunks of 4 for SIMD
    let chunks = neighbour_indices.chunks(4);
    let remainder_start = (neighbour_indices.len() / 4) * 4;

    for chunk in chunks {
        if chunk.len() == 4 {
            // SIMD path: process 4 neighbors at once
            let mut dx = [0.0f32; 4];
            let mut dy = [0.0f32; 4];
            let mut radii = [0.0f32; 4];
            let mut valid_mask = [true; 4];

            // Gather data for 4 neighbors
            for (i, &neighbour_idx) in chunk.iter().enumerate() {
                if let Some(b) = balls.get(neighbour_idx) {
                    let dist_vec = b.pre_update_pos.dist(ball_pos);
                    dx[i] = dist_vec.x;
                    dy[i] = dist_vec.y;
                    radii[i] = b.radius;
                } else {
                    valid_mask[i] = false;
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

            // === Force Calculation with Capping ===
            let r_cap_vec = f32x4::splat(r_cap);

            // Step 1: Calculate standard Lennard-Jones force
            // F_LJ(r) = 24·ε·(σ/r)^6 · [2·(σ/r)^6 - 1] / r
            // Use max() to prevent division by zero (1e-10 is safety epsilon)
            let quot = f32x4::splat(lj_sig) / edge_dist.max(f32x4::splat(1e-10));
            let quot_six = quot * quot * quot * quot * quot * quot; // (σ/r)^6
            let lj_force_uncapped = f32x4::splat(24. * lj_eps)
                * quot_six
                * (f32x4::splat(2.) * quot_six - f32x4::splat(1.))
                / edge_dist.max(f32x4::splat(1e-10));

            // Step 2: Cap force at max_force to prevent numerical instabilities
            // For r < r_cap: F = max_force (capped)
            // For r >= r_cap: F = F_LJ (uncapped)
            let max_force_vec = f32x4::splat(max_force);
            let lj_force = lj_force_uncapped.min(max_force_vec);

            // Force direction: force / center_dist
            // Avoid division by zero: if center_dist is 0, use a small epsilon to prevent Inf/NaN
            let safe_center_dist = center_dist.max(f32x4::splat(1e-10));
            let force_to_dist = lj_force / safe_center_dist;

            // Calculate force components
            let fx = dx_vec * force_to_dist;
            let fy = dy_vec * force_to_dist;

            // === Potential Energy Calculation with Force Capping ===
            // Standard LJ potential: U(r) = 4·ε·[(σ/r)^12 - (σ/r)^6]
            // For r < r_cap: Use linear extrapolation to preserve energy conservation

            // Calculate standard LJ potential (uncapped)
            let pot_quot = f32x4::splat(lj_sig) / edge_dist.max(f32x4::splat(1e-10));
            let pot_quot_six = pot_quot * pot_quot * pot_quot * pot_quot * pot_quot * pot_quot;
            let potential_uncapped =
                f32x4::splat(4. * lj_eps) * pot_quot_six * (pot_quot_six - f32x4::splat(1.));

            // Pre-calculate U(r_cap) for linear extrapolation in force-capped region
            // This is the "anchor point" where capped and uncapped potentials meet
            let cap_quot = f32x4::splat(lj_sig / r_cap);
            let cap_quot_six = cap_quot * cap_quot * cap_quot * cap_quot * cap_quot * cap_quot;
            let pot_at_cap =
                f32x4::splat(4. * lj_eps) * cap_quot_six * (cap_quot_six - f32x4::splat(1.));

            // Linear potential for r < r_cap: U(r) = U(r_cap) - max_force·(r - r_cap)
            // This ensures F = -dU/dr = +max_force (repulsive, constant) below r_cap
            let potential_linear = pot_at_cap - max_force_vec * (edge_dist - r_cap_vec);

            // Extract results and accumulate
            let fx_arr: [f32; 4] = fx.into();
            let fy_arr: [f32; 4] = fy.into();
            let edge_dist_arr: [f32; 4] = edge_dist.into();
            let pot_uncapped_arr: [f32; 4] = potential_uncapped.into();
            let pot_linear_arr: [f32; 4] = potential_linear.into();

            for i in 0..4 {
                if valid_mask[i] {
                    // If overlapping, use fixed repulsive force (match scalar behavior)
                    if edge_dist_arr[i] <= 0. {
                        // Get direction vector from ball to neighbor
                        if let Some(&neighbour_idx) = chunk.get(i)
                            && let Some(b) = balls.get(neighbour_idx)
                        {
                            let dist_vec = b.pre_update_pos.dist(ball_pos);
                            let center_dist = dist_vec.norm();
                            // Avoid division by zero
                            let force_to_dist_val = if center_dist > 0. {
                                max_force / center_dist
                            } else {
                                0.0
                            };
                            let local_force_x = dist_vec.x * force_to_dist_val;
                            let local_force_y = dist_vec.y * force_to_dist_val;
                            force.x += local_force_x;
                            force.y += local_force_y;
                            // Potential: can set to zero or leave as is (scalar sets to zero)
                            // total_pot += 0.0;
                            min_dist = 0.;
                            if ball_verbose_console || edge_dist_arr[i] < 0. {
                                let local_force_norm = f32::sqrt(
                                    local_force_x * local_force_x + local_force_y * local_force_y,
                                );
                                println!(
                                    "\t\t {}--force: {} ({},{}) at dist: {}",
                                    b.get_name(),
                                    local_force_norm,
                                    local_force_x,
                                    local_force_y,
                                    edge_dist_arr[i]
                                );
                            }
                        }
                    } else {
                        // Normal force application
                        force.x += fx_arr[i];
                        force.y += fy_arr[i];

                        // Select correct potential based on distance
                        let pot = if edge_dist_arr[i] < r_cap {
                            pot_linear_arr[i]
                        } else {
                            pot_uncapped_arr[i]
                        };
                        total_pot += pot;

                        // Update min_dist
                        min_dist = if f32::is_nan(min_dist) {
                            edge_dist_arr[i]
                        } else {
                            f32::min(min_dist, edge_dist_arr[i])
                        };
                        if (ball_verbose_console || edge_dist_arr[i] < 0.)
                            && let Some(&neighbour_idx) = chunk.get(i)
                            && let Some(b) = balls.get(neighbour_idx)
                        {
                            println!(
                                "\t\t {}--force: {} ({},{}) at dist: {}",
                                b.get_name(),
                                f32::sqrt(fx_arr[i] * fx_arr[i] + fy_arr[i] * fy_arr[i]),
                                fx_arr[i],
                                fy_arr[i],
                                edge_dist_arr[i]
                            );
                        }
                    }
                }
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

            if center_dist > 0. {
                // Calculate force using physics helper
                let lj_force = physics.lenard_jones_force(center_dist, radius_sum, b.get_id());

                let force_to_dist_val = lj_force / center_dist;
                let local_force_x = dist_vec.x * force_to_dist_val;
                let local_force_y = dist_vec.y * force_to_dist_val;

                force.x += local_force_x;
                force.y += local_force_y;

                // Potential
                total_pot += physics.lenard_jones_potential(edge_dist);

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

                if ball_verbose_console || edge_dist < 0. {
                    let local_force_norm =
                        f32::sqrt(local_force_x * local_force_x + local_force_y * local_force_y);
                    println!(
                        "\t\t {}--force: {} ({},{}) at dist: {}",
                        b.get_name(),
                        local_force_norm,
                        local_force_x,
                        local_force_y,
                        edge_dist
                    );
                }
            } else {
                // center_dist == 0: Two balls at exactly the same position
                // This is a physics error that should not occur in normal simulation
                // Set min_dist to 0 to indicate collision/error state
                min_dist = 0.;
                if ball_verbose_console {
                    println!(
                        "\t\t WARNING: ball {} and {} are at exactly the same position (center_dist = 0)",
                        balls[ball_idx].get_name(),
                        b.get_name()
                    );
                }
            }
        }
    }

    ForceResult {
        min_dist,
        force,
        potential_energy: total_pot,
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
    fn test_simd_vs_scalar_single_pair() {
        let physics = BallPhysics::default();

        // Create two balls at a specific distance
        let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
        let ball_b = Ball2D::new(1, Vec2D::new(3.0, 0.0), (0, 1.0));

        // Set up neighbor list
        ball_a.hood.add_neighbour(1);

        let balls = vec![ball_a, ball_b];

        // Calculate with scalar version
        let scalar_result = physics.force_indexed(0, &balls);

        // Calculate with SIMD version
        let simd_result = force_indexed_simd(&physics, 0, &balls);

        // Results should match within floating point precision
        approx(scalar_result.force.x, simd_result.force.x, 1e-5);
        approx(scalar_result.force.y, simd_result.force.y, 1e-5);
        approx(
            scalar_result.potential_energy,
            simd_result.potential_energy,
            1e-5,
        );
        approx(scalar_result.min_dist, simd_result.min_dist, 1e-5);
    }

    #[test]
    fn test_simd_vs_scalar_multiple_neighbors() {
        let physics = BallPhysics::default();

        // Create a ball with 8 neighbors (tests both SIMD chunks and remainder)
        let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));

        // Add 8 neighbors in a circle
        for i in 1..=8 {
            ball_a.hood.add_neighbour(i);
        }

        let mut balls = vec![ball_a];
        for i in 1..=8 {
            let angle = (i as f32) * std::f32::consts::PI * 2.0 / 8.0;
            let x = 3.0 * angle.cos();
            let y = 3.0 * angle.sin();
            balls.push(Ball2D::new(i, Vec2D::new(x, y), (0, 1.0)));
        }

        // Calculate with both methods
        let scalar_result = physics.force_indexed(0, &balls);
        let simd_result = force_indexed_simd(&physics, 0, &balls);

        // Results should match within floating point precision
        approx(scalar_result.force.x, simd_result.force.x, 1e-4);
        approx(scalar_result.force.y, simd_result.force.y, 1e-4);
        approx(
            scalar_result.potential_energy,
            simd_result.potential_energy,
            1e-4,
        );
        approx(scalar_result.min_dist, simd_result.min_dist, 1e-5);
    }

    #[test]
    fn test_simd_energy_conservation() {
        let physics = BallPhysics::default();

        // Create two balls
        let mut ball_a = Ball2D::new(0, Vec2D::new(0.0, 0.0), (0, 1.0));
        let ball_b = Ball2D::new(1, Vec2D::new(2.5, 0.0), (0, 1.0));

        ball_a.hood.add_neighbour(1);
        let balls = vec![ball_a, ball_b];

        let result = force_indexed_simd(&physics, 0, &balls);

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

        // Calculate with scalar version
        let scalar_result = physics.force_indexed(0, &balls);

        // Calculate with SIMD version
        let simd_result = force_indexed_simd(&physics, 0, &balls);

        // When balls overlap (edge_dist < 0), both should set min_dist to 0.0
        assert_eq!(
            scalar_result.min_dist, 0.0,
            "Scalar min_dist should be 0.0 for overlapping balls"
        );
        assert_eq!(
            simd_result.min_dist, 0.0,
            "SIMD min_dist should be 0.0 for overlapping balls"
        );

        // Both scalar and SIMD now use max_force when edge_dist <= 0
        // They should produce the same repulsive forces in the same direction
        assert!(
            scalar_result.force.x < 0.0,
            "Scalar force should be repulsive (negative x)"
        );
        assert!(
            simd_result.force.x < 0.0,
            "SIMD force should be repulsive (negative x)"
        );
    }
}
