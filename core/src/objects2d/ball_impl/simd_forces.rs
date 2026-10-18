// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! SIMD-optimized pair force calculations
//!
//! This module provides the vectorized force kernel using portable `std::simd`
//! (`f32x4`, 4 neighbours per chunk, scalar code for the remainder). The pair
//! interaction itself (law, force cap, cutoff shift, one potential per species
//! pair) lives in [`pair_potential`](super::pair_potential). Distances are
//! centre distances; each chunk gathers the potentials of its 4 pairs from the
//! ball's row of the pair table, by the neighbours' species.
//!
//! There is no runtime dispatch or multiversioning: the instruction set is fixed at
//! compile time by the target features. The dev profile uses the target's baseline
//! (SSE2 on x86_64, NEON on aarch64); the release profile adds `target-cpu=native`
//! (`[profile.release]` in the workspace `Cargo.toml`), so it uses whatever the build
//! host supports (e.g. AVX2/FMA). f32 results can differ between the two.
//!
//! # Force Capping for Stability
//! To prevent numerical instabilities when particles get very close, forces are capped
//! at `max_force`. This is implemented as:
//! - For r >= r_cap: Standard LJ force F_LJ(r)
//! - For r < r_cap: max_force + core_stiffness·(r_cap - r), rising with the overlap
//! - For r >= lj_cutoff: no interaction
//!
//! The corresponding potential energy uses the quadratic continuation
//! U(r) = U(r_cap) - max_force·(r - r_cap) + ½·core_stiffness·(r - r_cap)² below r_cap and is shifted to zero at the cutoff, so neighbour-list changes (which only
//! happen beyond the cutoff) do not change the energy. See `CappedPotential` for details.

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::pair_potential::{
    CappedLanes, PairLaw, PairPotential, PairTable, Potential,
};
use crate::objects2d::ball_impl::{Ball2D, BallPhysics, ForceResult};
use std::simd::prelude::SimdFloat;
use std::simd::{StdFloat, f32x4};

/// The 16 bytes of a ball the force kernel reads for every neighbour: position
/// (`pre_update_pos`), species and contact radius.
///
/// Neighbour lookups are random accesses, so one packed 16-byte record (4 per
/// cache line) costs one cache line per neighbour, where a full `Ball2D` or
/// separate arrays would cost one or more. Rebuilt every step by
/// `KernelPoint::from`, so it never holds indices that could go stale.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C, align(16))]
pub struct KernelPoint {
    pub x: f32,
    pub y: f32,
    /// Index of the ball's species.
    pub species: u32,
    /// `σ / 2` of the species, for the overlap gap in `ForceResult::min_dist`.
    pub contact_radius: f32,
}

impl From<&Ball2D> for KernelPoint {
    #[inline]
    fn from(ball: &Ball2D) -> Self {
        Self {
            x: ball.pre_update_pos.x,
            y: ball.pre_update_pos.y,
            species: ball.species().index() as u32,
            contact_radius: ball.contact_radius,
        }
    }
}

impl KernelPoint {
    /// Vector from this point to `to` (same as `Vec2D::dist`: `to - self`).
    #[inline]
    fn dist(&self, to: &Vec2D) -> Vec2D {
        Vec2D::new(to.x - self.x, to.y - self.y)
    }
}

/// `KernelPoint`s for all balls, as the simulation step builds them.
#[cfg(test)]
pub(crate) fn kernel_points(balls: &[Ball2D]) -> Vec<KernelPoint> {
    balls.iter().map(KernelPoint::from).collect()
}

/// Calculate Lennard-Jones forces for multiple ball pairs using SIMD vectorization.
/// Processes neighbors in chunks of 4 with `f32x4` (see the module docs for the instruction set).
/// Below r_cap the force rises linearly from max_force, with the matching quadratic potential.
/// Conditionally collects per-neighbor force magnitudes only when visualization is enabled.
///
/// # Arguments
/// * `physics` - Physics engine with the pair potentials and force cap settings
/// * `ball_idx` - Index of ball to calculate forces for
/// * `balls` - Slice of all balls (read-only access)
/// * `points` - `KernelPoint` of every ball (same indices as `balls`), taken after `update_old_pos`
///
/// # Returns
/// ForceResult containing total force vector, potential energy, minimum distance, and optional neighbor forces
pub fn force_simd(
    physics: &BallPhysics,
    ball_idx: usize,
    balls: &[Ball2D],
    points: &[KernelPoint],
) -> ForceResult {
    // Static dispatch: one match per ball, outside the neighbour loop; the
    // kernel below is monomorphised for each law, with no branch per neighbour.
    match physics.pair_potential() {
        Potential::LennardJones(table) => force_kernel(table, ball_idx, balls, points),
    }
}

/// The force kernel of [`force_simd`] for one concrete pair law.
#[inline]
fn force_kernel<L: PairLaw>(
    table: &PairTable<L>,
    ball_idx: usize,
    balls: &[Ball2D],
    points: &[KernelPoint],
) -> ForceResult {
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

    let ball = &balls[ball_idx];
    let ball_pos = ball.get_position();
    let ball_contact = ball.contact_radius;
    // This ball's potential with each species.
    let pairs = table.row(ball.species());
    let neighbour_indices = ball.hood.get_neighbour_indices();

    // Process neighbors in chunks of 4 for SIMD
    let (chunks, unchunked) = neighbour_indices.as_chunks::<4>();

    for chunk in chunks {
        // SIMD path: process 4 neighbors at once
        let mut dx = [0.0f32; 4];
        let mut dy = [0.0f32; 4];
        let mut contacts = [0.0f32; 4];
        let mut chunk_pairs = [&pairs[0]; 4];

        // Gather data for 4 neighbors
        for (i, &neighbour_idx) in chunk.iter().enumerate() {
            if let Some(neighbour) = points.get(neighbour_idx) {
                let dist_vec = neighbour.dist(ball_pos);
                dx[i] = dist_vec.x;
                dy[i] = dist_vec.y;
                contacts[i] = neighbour.contact_radius;
                chunk_pairs[i] = &pairs[neighbour.species as usize];
            } else {
                panic!("Invalid neighbor index in SIMD force calculation");
            }
        }
        let potential = CappedLanes::gather(chunk_pairs);

        // Load into SIMD vectors
        let dx_vec = f32x4::from(dx);
        let dy_vec = f32x4::from(dy);

        // Calculate distances: sqrt(dx^2 + dy^2)
        let center_dist_sq = dx_vec * dx_vec + dy_vec * dy_vec;
        let center_dist = center_dist_sq.sqrt();

        // Pair force, sloped from max_force below r_cap and zero beyond the
        // cutoff (see CappedPotential).
        let lj_force = potential.force(center_dist);

        // Force direction: force / center_dist
        // Avoid division by zero: if center_dist is 0, use a small epsilon to prevent Inf/NaN
        let safe_center_dist = center_dist.simd_max(f32x4::splat(1e-10));
        let force_to_dist = lj_force / safe_center_dist;

        // Calculate force components
        let fx = dx_vec * force_to_dist;
        let fy = dy_vec * force_to_dist;

        // Shifted potential with quadratic continuation below r_cap, zero
        // beyond the cutoff (see CappedPotential).
        let potential_used = potential.potential(center_dist);

        // Virial: Σ (r_ij · F_ij).
        // Since fx = dx_vec * force_to_dist and fy = dy_vec * force_to_dist,
        // we have r · F = (dx^2 + dy^2) * force_to_dist = center_dist_sq * force_to_dist.
        virial += (center_dist_sq * force_to_dist).reduce_sum();

        force.x += fx.reduce_sum();
        force.y += fy.reduce_sum();
        total_pot += potential_used.reduce_sum();
        let gap = center_dist - (f32x4::from(contacts) + f32x4::splat(ball_contact));
        let min_dist_of_chunk = gap.reduce_min();
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
        if let Some(b) = points.get(neighbour_idx) {
            let dist_vec = b.dist(ball_pos);
            let center_dist = dist_vec.norm();
            let gap = center_dist - (b.contact_radius + ball_contact);
            let potential = &pairs[b.species as usize];

            let lj_force = potential.force(center_dist);

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

            total_pot += potential.potential(center_dist);

            // Store per-neighbor force magnitude for visualization
            if collect_neighbor_forces {
                neighbor_forces.push((neighbour_idx, lj_force.abs()));
            }
            min_dist = if f32::is_nan(min_dist) {
                gap
            } else {
                f32::min(min_dist, gap)
            };
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
