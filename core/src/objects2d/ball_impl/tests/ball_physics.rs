// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::ball_physics::BallPhysics;
use crate::objects2d::ball_impl::pair_potential::{LennardJones, PairLaw};
use crate::objects2d::ball_impl::pair_potential::{PairPotential, Potential};
use crate::objects2d::ball_impl::{Ball2D, DEFAULT_SPECIES, SpeciesId, SpeciesTable};

/// The reference species (`σ = ε = 1`): its pair is plain Lennard-Jones in
/// reduced units.
const N2: SpeciesId = DEFAULT_SPECIES;

fn force(phys: &BallPhysics, r: f32) -> f32 {
    phys.force(N2, N2, r)
}

fn potential(phys: &BallPhysics, r: f32) -> f32 {
    phys.potential(N2, N2, r)
}

fn r_cap(phys: &BallPhysics) -> f32 {
    phys.get_force_cap_distance(N2, N2)
}

fn cutoff(phys: &BallPhysics) -> f32 {
    phys.cutoff(N2, N2)
}

fn species(name: &str) -> SpeciesId {
    SpeciesTable::builtin().by_name(name).unwrap()
}

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
fn test_lj_equilibrium() {
    let phys = BallPhysics::default();
    let r_eq = 1.122_462_f32; // 2^(1/6)
    let pot = potential(&phys, r_eq);

    // -ε, shifted so the potential is zero at the cutoff
    let pot_at_cutoff = LennardJones::default().potential(cutoff(&phys));
    approx(pot, -1.0 - pot_at_cutoff, 1e-5);
    let force = force(&phys, r_eq);

    approx(force, 0.0, 1e-5);
}

/// Unlike species interact with the mixed `σ_ij = (σ_i + σ_j) / 2`: the
/// force vanishes at `2^(1/6)·σ_ij` (centre distance), whatever the order.
#[test]
fn test_mixed_pair_equilibrium_is_at_mixed_sigma() {
    let phys = BallPhysics::default();
    let table = SpeciesTable::builtin();
    let (n2, xl) = (N2, species("XL"));
    let sigma_ij = 0.5 * (table.species(n2).sigma + table.species(xl).sigma);
    let r_eq = 1.122_462_f32 * sigma_ij;
    approx(phys.equilibrium_distance(n2, xl), r_eq, 1e-6);
    approx(phys.force(n2, xl, r_eq), 0.0, 1e-4);
    assert_eq!(
        phys.force(n2, xl, 0.9 * r_eq),
        phys.force(xl, n2, 0.9 * r_eq)
    );
    approx(phys.cutoff(n2, xl), 4.0 * sigma_ij, 1e-6);
}

#[test]
fn test_is_colliding_detection() {
    let phys = BallPhysics::default();
    // Contact discs of radius σ/2 touch at centre distance σ_ij.
    let m = species("M");
    let sigma_ij = 0.5 * (1.0 + SpeciesTable::builtin().species(m).sigma);
    let a = Ball2D::new(1, Vec2D::new(2.0, 3.0), N2);

    let b = Ball2D::new(2, Vec2D::new(2.0 + sigma_ij * 0.99999, 3.0), m);

    assert!(phys.is_colliding(&a, &b, 0.));

    let c = Ball2D::new(2, Vec2D::new(2.0, 3.0 + sigma_ij * 1.00001), m);

    assert!(!phys.is_colliding(&a, &c, 0.));
}

#[test]
fn test_lenard_jones_force_when_close() {
    let phys = BallPhysics::default();
    let r_eq = 1.122_462_f32; // 2^(1/6) * sigma with sigma=1
    // Slightly below equilibrium -> repulsive -> positive force magnitude returned by pure
    let f_near = force(&phys, r_eq * 0.9);
    // Slightly above equilibrium -> attractive -> negative force
    for i in 1..30 {
        let r = i as f32 * r_eq * 0.1;
        let f = force(&phys, r);
        println!("r {r}  force: {f}");
    }

    let f_far = force(&phys, 3.0 * r_eq);
    assert!(
        f_near > 0.0,
        "Expected repulsive (positive) force when nearer than equilibrium, got {}",
        f_near
    );
    assert!(
        f_far < 0.0,
        "Expected attractive (negative) force when farther than equilibrium, got {}",
        f_far
    );
}

#[test]
fn test_min_movement_threshold_formula() {
    let phys = BallPhysics::default();

    // Test formula: min_movement = skin_thickness / 3, so that per-ball,
    // asynchronous rebuilds still keep every in-cutoff pair listed:
    // cutoff + skin - 3 * threshold >= cutoff.
    let skin_thickness = phys.get_skin_thickness();
    let expected_threshold = skin_thickness / 3.0;

    let actual_threshold = phys.get_min_movement_for_update_hood();

    approx(actual_threshold, expected_threshold, 1e-6);
}

#[test]
fn test_force_cap_at_max_force() {
    let phys = BallPhysics::default();
    let max_force = phys.cap_settings().max_force;
    let stiffness = phys.cap_settings().core_stiffness;
    let r_cap = r_cap(&phys);

    // At r_cap, force should be approximately max_force
    let force_at_cap = force(&phys, r_cap);
    approx(force_at_cap, max_force, 1e-2);

    // Below r_cap, the force rises linearly from max_force
    for r in [r_cap * 0.5, r_cap * 0.1] {
        approx(force(&phys, r), max_force + stiffness * (r_cap - r), 1e-2);
    }
}

#[test]
fn test_potential_quadratic_below_cap() {
    let phys = BallPhysics::default();
    let max_force = phys.cap_settings().max_force;
    let stiffness = phys.cap_settings().core_stiffness;
    let r_cap = r_cap(&phys);

    let pot_at_cap = potential(&phys, r_cap);

    // Expected potential: U(r) = U(r_cap) - max_force·(r - r_cap) + ½·k·(r - r_cap)²
    for r in [r_cap * 0.8, r_cap * 0.5, r_cap * 0.05] {
        let expected = pot_at_cap - max_force * (r - r_cap) + 0.5 * stiffness * (r - r_cap).powi(2);
        approx(potential(&phys, r), expected, 1e-4);
    }
}

#[test]
fn test_force_not_capped_above_threshold() {
    let phys = BallPhysics::default();
    let max_force = phys.cap_settings().max_force;
    let r_cap = r_cap(&phys);

    // Above r_cap, force should follow normal LJ formula
    let r_above = r_cap * 2.0;
    let force_above = force(&phys, r_above);

    // Force should be less than max_force
    assert!(
        force_above < max_force,
        "Force above cap distance should be less than max_force"
    );
}

#[test]
fn test_energy_conservation_with_force_cap() {
    // Below r_cap the force must be -dU/dr of the potential, or capped
    // contacts gain or lose energy.
    let phys = BallPhysics::default();
    let r_cap = r_cap(&phys);
    let h = 1e-3;

    for r in [r_cap * 0.9, r_cap * 0.5, r_cap * 0.2, r_cap * 0.05] {
        let slope = (potential(&phys, r + h) - potential(&phys, r - h)) / (2. * h);
        approx(force(&phys, r), -slope, 1e-2);
    }
}

#[test]
fn test_energy_conservation_two_balls_integration() {
    // Integration test: Simulate two interacting balls for 1000 steps
    // and verify total energy (kinetic + potential) remains constant within numerical error

    use crate::objects2d::ball_impl::ball_factory::BallFactory;
    use crate::objects2d::ball_impl::simd_forces::{force_simd, kernel_points};

    let phys = BallPhysics::default();
    let factory = BallFactory::new();

    // Create two balls in close proximity to trigger strong forces
    // Position them at a distance where force capping will be active
    let r_cap = r_cap(&phys);
    let separation = r_cap * 0.8; // Centre distance inside the force cap

    let mut ball1 = factory.create_basic(Vec2D::new(0.0, 0.0), N2);
    let mut ball2 = factory.create_basic(Vec2D::new(separation, 0.0), N2);

    // Give them small initial velocities to make the interaction more interesting
    ball1.v = Vec2D::new(0.1, 0.05);
    ball2.v = Vec2D::new(-0.08, -0.03);

    // Initialize pre_update positions
    ball1.pre_update_pos = *ball1.get_position();
    ball2.pre_update_pos = *ball2.get_position();

    // Set up neighbor lists (each ball sees the other)
    ball1.hood.add_neighbour(1);
    ball2.hood.add_neighbour(0);

    let mut balls = vec![ball1, ball2];

    // Calculate initial energy
    let initial_energy = calculate_total_energy(&phys, &balls);

    println!("Initial energy: {:.6}", initial_energy);
    println!("Initial separation: {:.6}", separation);
    println!("Force cap distance (r_cap): {:.6}", r_cap);

    // Simulation parameters
    const DT: f32 = 0.001; // Small timestep for numerical stability
    const NUM_STEPS: usize = 1000;

    let mut max_energy_error = 0.0f32;
    let mut energy_at_max_error = 0.0f32;

    // Run simulation
    for step in 0..NUM_STEPS {
        // Compute forces using SIMD implementation
        let force1 = force_simd(&phys, 0, &balls, &kernel_points(&balls));
        let force2 = force_simd(&phys, 1, &balls, &kernel_points(&balls));

        // Update velocities (velocity Verlet integration - part 1)
        let accel1 = Vec2D::new(
            force1.force.x / balls[0].mass,
            force1.force.y / balls[0].mass,
        );
        let accel2 = Vec2D::new(
            force2.force.x / balls[1].mass,
            force2.force.y / balls[1].mass,
        );

        // Update velocities with half-step acceleration
        balls[0].v.x += 0.5 * accel1.x * DT;
        balls[0].v.y += 0.5 * accel1.y * DT;
        balls[1].v.x += 0.5 * accel2.x * DT;
        balls[1].v.y += 0.5 * accel2.y * DT;

        // Update positions
        let new_pos1 = Vec2D::new(
            balls[0].get_position().x + balls[0].v.x * DT,
            balls[0].get_position().y + balls[0].v.y * DT,
        );
        let new_pos2 = Vec2D::new(
            balls[1].get_position().x + balls[1].v.x * DT,
            balls[1].get_position().y + balls[1].v.y * DT,
        );

        balls[0].set_position_and_update_tail(new_pos1);
        balls[1].set_position_and_update_tail(new_pos2);
        balls[0].pre_update_pos = new_pos1;
        balls[1].pre_update_pos = new_pos2;

        // Recompute forces at new positions
        let force1_new = force_simd(&phys, 0, &balls, &kernel_points(&balls));
        let force2_new = force_simd(&phys, 1, &balls, &kernel_points(&balls));

        // Complete velocity update with new accelerations (velocity Verlet - part 2)
        let accel1_new = Vec2D::new(
            force1_new.force.x / balls[0].mass,
            force1_new.force.y / balls[0].mass,
        );
        let accel2_new = Vec2D::new(
            force2_new.force.x / balls[1].mass,
            force2_new.force.y / balls[1].mass,
        );

        balls[0].v.x += 0.5 * accel1_new.x * DT;
        balls[0].v.y += 0.5 * accel1_new.y * DT;
        balls[1].v.x += 0.5 * accel2_new.x * DT;
        balls[1].v.y += 0.5 * accel2_new.y * DT;

        // Check energy conservation every 100 steps
        if step % 100 == 0 {
            let current_energy = calculate_total_energy(&phys, &balls);
            let energy_error = (current_energy - initial_energy).abs();
            let rel_error = energy_error / initial_energy.abs();

            if energy_error > max_energy_error {
                max_energy_error = energy_error;
                energy_at_max_error = current_energy;
            }

            if step % 200 == 0 {
                println!(
                    "Step {}: Energy = {:.6}, Error = {:.6} ({:.4}%)",
                    step,
                    current_energy,
                    energy_error,
                    rel_error * 100.0
                );
            }
        }
    }

    // Final energy check
    let final_energy = calculate_total_energy(&phys, &balls);
    let total_energy_drift = (final_energy - initial_energy).abs();
    let relative_drift = total_energy_drift / initial_energy.abs();

    println!("\n=== Energy Conservation Results ===");
    println!("Initial energy:    {:.6}", initial_energy);
    println!("Final energy:      {:.6}", final_energy);
    println!(
        "Max energy error:  {:.6} (at E = {:.6})",
        max_energy_error, energy_at_max_error
    );
    println!("Total drift:       {:.6}", total_energy_drift);
    println!(
        "Relative drift:    {:.6}% ({:.2e})",
        relative_drift * 100.0,
        relative_drift
    );

    // Energy should be conserved within numerical precision
    // For 1000 steps with dt=0.001, we expect some drift due to:
    // 1. Floating point rounding errors accumulating
    // 2. Velocity Verlet integration error (O(dt^2))
    // 3. Force capping discontinuity (though potential is continuous)

    // Accept up to 1% relative error for this test
    // This is a fairly loose tolerance, but force capping at close distances
    // can introduce some numerical challenges
    const MAX_ACCEPTABLE_DRIFT: f32 = 0.01; // 1%

    assert!(
        relative_drift < MAX_ACCEPTABLE_DRIFT,
        "Energy drift too large: {:.4}% (max acceptable: {:.4}%). \
         This indicates energy is not being conserved properly with force capping. \
         Initial: {:.6}, Final: {:.6}, Drift: {:.6}",
        relative_drift * 100.0,
        MAX_ACCEPTABLE_DRIFT * 100.0,
        initial_energy,
        final_energy,
        total_energy_drift
    );

    println!("✓ Energy conservation test PASSED");
}

/// Helper function to calculate total energy (kinetic + potential) of a system
fn calculate_total_energy(phys: &BallPhysics, balls: &[Ball2D]) -> f32 {
    // Calculate kinetic energy
    let mut kinetic = 0.0f32;
    for ball in balls {
        let speed_sq = ball.v.x * ball.v.x + ball.v.y * ball.v.y;
        kinetic += 0.5 * ball.mass * speed_sq;
    }

    // Calculate potential energy (only count each pair once)
    let mut potential = 0.0f32;
    for i in 0..balls.len() {
        for j in (i + 1)..balls.len() {
            let dist_vec = balls[j].get_position().dist(balls[i].get_position());
            let center_dist = dist_vec.norm();

            // Use the physics engine's potential calculation (includes force capping)
            let pair_potential =
                phys.potential(balls[i].species(), balls[j].species(), center_dist);
            potential += pair_potential;
        }
    }

    kinetic + potential
}

#[test]
fn test_lj_force_and_potential_above_rcap() {
    // Verify that Lennard-Jones calculations are correct for distances >= r_cap
    let phys = BallPhysics::default();
    let r_cap = r_cap(&phys);

    println!("\n=== Testing LJ Calculations Above r_cap ===");
    println!("r_cap = {:.6}\n", r_cap);

    // Test distances above r_cap
    let test_distances = vec![
        r_cap * 1.0, // At r_cap
        r_cap * 1.1, // 10% above
        r_cap * 1.5, // 50% above
        r_cap * 2.0, // 2x r_cap
        1.122462,    // Equilibrium (2^(1/6))
        r_cap * 3.0, // 3x r_cap
    ];

    // Verify F = -dU/dr numerically for distances above r_cap
    println!("\nVerifying F = -dU/dr above r_cap:");
    println!("Distance (r) | F (direct) | -dU/dr (numerical) | Error");
    println!("{}", "-".repeat(70));

    for &r in &test_distances {
        if r >= r_cap {
            let dr = 1e-4;
            let f_direct = force(&phys, r);
            let u1 = potential(&phys, r - dr / 2.0);
            let u2 = potential(&phys, r + dr / 2.0);
            let f_numerical = -(u2 - u1) / dr;
            let error = (f_direct - f_numerical).abs();

            println!(
                "{:12.6} | {:10.4} | {:18.4} | {:.4e}",
                r, f_direct, f_numerical, error
            );

            // They should match very closely
            approx(f_direct, f_numerical, 0.01);
        }
    }

    println!("✓ All LJ calculations above r_cap are correct");
}

#[test]
fn test_potential_continuity_at_rcap() {
    // Verify potential is continuous at the r_cap boundary
    let phys = BallPhysics::default();
    let r_cap = r_cap(&phys);
    let max_force = phys.cap_settings().max_force;

    println!("\n=== Testing Potential Continuity at r_cap ===");

    // Test continuity of potential at r_cap boundary
    let r_below = r_cap * 0.9999;
    let r_above = r_cap * 1.0001;

    let u_below = potential(&phys, r_below);
    let u_at = potential(&phys, r_cap);
    let u_above = potential(&phys, r_above);

    println!("r_cap = {:.10}", r_cap);
    println!("U(r_cap - ε) = {:.10}", u_below);
    println!("U(r_cap)     = {:.10}", u_at);
    println!("U(r_cap + ε) = {:.10}", u_above);

    // The potential should be continuous (smooth transition)
    let diff_below = (u_below - u_at).abs();
    let diff_above = (u_at - u_above).abs();

    println!("Difference below-at: {:.10e}", diff_below);
    println!("Difference at-above: {:.10e}", diff_above);

    // Both differences should be very small and similar
    assert!(
        diff_below < 0.01,
        "Discontinuity below r_cap: {}",
        diff_below
    );
    assert!(
        diff_above < 0.01,
        "Discontinuity above r_cap: {}",
        diff_above
    );

    // The slope should be approximately -max_force
    let slope_below = (u_at - u_below) / (r_cap - r_below);
    let slope_above = (u_above - u_at) / (r_above - r_cap);

    println!(
        "Slope below r_cap: {:.4} (should be ≈ -max_force = {:.4})",
        slope_below, -max_force
    );
    println!("Slope above r_cap: {:.4}", slope_above);

    // Below r_cap: slope should be -max_force (linear region)
    approx(slope_below, -max_force, 1.0);

    // Above r_cap: slope should transition smoothly
    assert!(
        (slope_above - slope_below).abs() < 50.0,
        "Slope discontinuity at r_cap: {} vs {}",
        slope_below,
        slope_above
    );

    println!("✓ Potential is continuous at r_cap boundary");
}

#[test]
fn test_overlapping_balls_force_capped() {
    // Deeply overlapping balls must not see the steep LJ wall (an overlap
    // once produced force = 1840), but the force must keep rising with the
    // overlap so crowded neighbours cannot pin a deeply overlapping pair
    // (issue #240).
    let phys = BallPhysics::default();
    let max_force = phys.cap_settings().max_force;
    let stiffness = phys.cap_settings().core_stiffness;
    let r_cap = r_cap(&phys);

    let dist = 0.3;
    approx(
        force(&phys, dist),
        max_force + stiffness * (r_cap - dist),
        1e-2,
    );

    let mut previous = force(&phys, r_cap);
    for dist in [0.8, 0.5, 0.2, 0.0] {
        let f = force(&phys, dist);
        assert!(f.is_finite(), "force at {dist} is {f}");
        assert!(
            f > previous,
            "force must grow with overlap: {f} <= {previous} at {dist}"
        );
        previous = f;
    }
}

#[test]
fn test_force_and_potential_vanish_at_and_beyond_cutoff() {
    let phys = BallPhysics::default();
    let cutoff = cutoff(&phys);
    for r in [cutoff, cutoff + 0.1, cutoff + 0.8, 100.0] {
        assert_eq!(force(&phys, r), 0.0, "force at {r}");
        assert_eq!(potential(&phys, r), 0.0, "potential at {r}");
    }
    // Continuous at the cutoff: no energy jump when a pair crosses it.
    let just_inside = potential(&phys, cutoff - 1e-3);
    assert!(just_inside.abs() < 1e-5, "U(r_c-) = {just_inside}");
    // Still attractive just inside the cutoff.
    assert!(force(&phys, cutoff - 1e-3) < 0.0);
}

#[test]
fn test_simd_force_and_potential_match_scalar_across_cutoff() {
    use std::simd::f32x4;
    let phys = BallPhysics::default();
    let cutoff = cutoff(&phys);
    let r_cap = r_cap(&phys);
    let Potential::LennardJones(table) = phys.pair_potential();
    let pair = table.pair(N2, N2);
    for dists in [
        [r_cap * 0.5, 1.0, 2.0, cutoff - 1e-3],
        [cutoff, cutoff + 0.1, cutoff + 0.8, 10.0],
    ] {
        let simd_force = pair.force_simd(f32x4::from(dists)).to_array();
        let simd_pot = pair.potential_simd(f32x4::from(dists)).to_array();
        for (i, &r) in dists.iter().enumerate() {
            approx(simd_force[i], force(&phys, r), 1e-6);
            approx(simd_pot[i], potential(&phys, r), 1e-6);
        }
    }
}
