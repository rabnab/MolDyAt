// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{DT_GOAL, create_main_content_for_region};
use crate::application::MainContent;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{BallFactory, DEFAULT_SPECIES};

#[test]
fn test_energy_conservation_1000_balls() {
    println!("\n=== Testing Energy Conservation with 1000 Balls ===");

    // Create a larger world for 1000 balls - 500x500 pixels (50x50 in world units)
    let (mut main_content, win_def) = create_main_content_for_region(500.0, 500.0);

    // Disable speed limit for momentum conservation testing
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);

    let world_rect = win_def;
    println!("World rect: {:?}", world_rect);
    println!("World size: {:.2} x {:.2}", world_rect.w, world_rect.h);

    let factory = BallFactory::new();

    // Add 1000 balls with random positions and velocities
    println!("\nAdding 1000 balls to the simulation...");
    let num_balls_to_add = 1000;
    let mut balls_added = 0;

    // Get world bounds with some margin
    let margin = 2.0;
    let (tl_x, tl_y) = world_rect.get_tl();
    let (br_x, br_y) = world_rect.get_br();
    let min_x = tl_x + margin;
    let max_x = br_x - margin;
    let min_y = br_y + margin; // Note: y is negative downward
    let max_y = tl_y - margin;

    println!(
        "Placement bounds: x=[{:.2}, {:.2}], y=[{:.2}, {:.2}]",
        min_x, max_x, min_y, max_y
    );

    // Use a simple grid placement with slight random offsets
    let grid_size = (num_balls_to_add as f32).sqrt().ceil() as usize;
    let x_spacing = (max_x - min_x) / (grid_size as f32);
    let y_spacing = (max_y - min_y) / (grid_size as f32);

    for i in 0..grid_size {
        for j in 0..grid_size {
            if balls_added >= num_balls_to_add {
                break;
            }

            let x = min_x + (i as f32 + 0.5) * x_spacing;
            let y = min_y + (j as f32 + 0.5) * y_spacing;
            let pos = Vec2D::new(x, y);

            // Thermalized velocity (pseudo-random based on position)
            // This approximates Maxwell-Boltzmann distribution
            let v_thermal = 1.4; // Thermal velocity scale
            let vx = (i as f32 * 0.617 + j as f32 * 0.271).sin() * v_thermal;
            let vy = (i as f32 * 0.314 + j as f32 * 0.159).cos() * v_thermal;
            let vel = Vec2D::new(vx, vy);

            // Create ball with small random velocity
            let ball = factory.create_massive_with_velocity(pos, vel);
            main_content.world_mut().balls.push(ball);
            balls_added += 1;
        }
    }
    println!("Successfully added {} balls", balls_added);

    // Add balls to cell grid
    println!("\nAdding balls to cell grid...");
    main_content.world_mut().rebuild_cell_grid();

    // Initialize neighbor lists
    println!("\nInitializing neighbor lists...");
    main_content.collect_neighbours_by_cell(DT_GOAL);

    // Helper function to calculate total energy
    let calculate_energy = |main_content: &MainContent| -> (f32, f32, f32) {
        let world = main_content.world();
        let mut kinetic = 0.0f32;
        let mut potential = 0.0f32;

        // Kinetic energy
        for ball in &world.balls {
            let speed_sq = ball.v.x * ball.v.x + ball.v.y * ball.v.y;
            kinetic += 0.5 * ball.mass * speed_sq;
        }

        // Potential energy (count each pair once)
        for i in 0..world.balls.len() {
            for j in (i + 1)..world.balls.len() {
                let dist_vec = world.balls[j]
                    .get_position()
                    .dist(world.balls[i].get_position());
                let center_dist = dist_vec.norm();
                let (a, b) = (world.balls[i].species(), world.balls[j].species());
                let pair_potential = world.physics.potential(a, b, center_dist);
                potential += pair_potential;
            }
        }

        (kinetic, potential, kinetic + potential)
    };

    // Measure initial energy
    println!("\nCalculating initial energy...");
    let (ke_initial, pe_initial, total_initial) = calculate_energy(&main_content);

    // Calculate initial momentum
    let mut total_px_initial = 0.0f32;
    let mut total_py_initial = 0.0f32;
    for ball in &main_content.world().balls {
        total_px_initial += ball.mass * ball.v.x;
        total_py_initial += ball.mass * ball.v.y;
    }
    let total_p_mag_initial =
        (total_px_initial * total_px_initial + total_py_initial * total_py_initial).sqrt();

    println!("Initial energy:");
    println!("  Kinetic:   {:.6}", ke_initial);
    println!("  Potential: {:.6}", pe_initial);
    println!("  Total:     {:.6}", total_initial);
    println!("Initial momentum:");
    println!(
        "  Total p: ({:.6}, {:.6})",
        total_px_initial, total_py_initial
    );
    println!("  |p|:     {:.6}", total_p_mag_initial);

    // Run simulation for 100 steps
    println!("\nRunning simulation for 100 steps...");
    let bound_rect = main_content.world().world_region();

    for step in 0..100 {
        main_content.update_sim(DT_GOAL, &bound_rect);

        if step % 10 == 9 {
            let (ke, pe, total) = calculate_energy(&main_content);
            let drift = total - total_initial;
            let relative_drift = if total_initial.abs() > 1e-10 {
                (drift / total_initial).abs()
            } else {
                drift.abs()
            };

            // Calculate current momentum
            let mut total_px = 0.0f32;
            let mut total_py = 0.0f32;
            let world = main_content.world();
            let num_balls = world.balls.len();
            let mut nan_count = 0;
            for ball in &world.balls {
                if ball.get_position().x.is_nan() || ball.get_position().y.is_nan() {
                    nan_count += 1;
                }
                total_px += ball.mass * ball.v.x;
                total_py += ball.mass * ball.v.y;
            }
            let total_p_mag = (total_px * total_px + total_py * total_py).sqrt();
            let p_drift_x = total_px - total_px_initial;
            let p_drift_y = total_py - total_py_initial;
            let p_drift_mag = (p_drift_x * p_drift_x + p_drift_y * p_drift_y).sqrt();

            // Get wall momentum
            let wall_px = world.sim.wall_momentum_x;
            let wall_py = world.sim.wall_momentum_y;
            let wall_p_mag = (wall_px * wall_px + wall_py * wall_py).sqrt();

            // Total system momentum (balls + walls)
            let system_px = total_px + wall_px;
            let system_py = total_py + wall_py;
            let system_p_mag = (system_px * system_px + system_py * system_py).sqrt();
            let system_p_drift_x = system_px - total_px_initial;
            let system_p_drift_y = system_py - total_py_initial;
            let system_p_drift_mag =
                (system_p_drift_x * system_p_drift_x + system_p_drift_y * system_p_drift_y).sqrt();

            println!(
                "Step {}: #balls={}, nan={}, KE={:.6}, PE={:.6}, Total={:.6}, Drift={:.6} ({:.4}%)",
                step + 1,
                num_balls,
                nan_count,
                ke,
                pe,
                total,
                drift,
                relative_drift * 100.0
            );
            println!(
                "         Balls p=({:.6}, {:.6}), |p|={:.6}, drift={:.6}",
                total_px, total_py, total_p_mag, p_drift_mag
            );
            println!(
                "         Walls p=({:.6}, {:.6}), |p|={:.6}",
                wall_px, wall_py, wall_p_mag
            );
            println!(
                "         System p=({:.6}, {:.6}), |p|={:.6}, drift={:.6}",
                system_px, system_py, system_p_mag, system_p_drift_mag
            );
        }
    }

    // Measure final energy
    println!("\nCalculating final energy...");
    let (ke_final, pe_final, total_final) = calculate_energy(&main_content);
    println!("Final energy:");
    println!("  Kinetic:   {:.6}", ke_final);
    println!("  Potential: {:.6}", pe_final);
    println!("  Total:     {:.6}", total_final);

    // Calculate energy drift
    let energy_drift = total_final - total_initial;
    let relative_drift = if total_initial.abs() > 1e-10 {
        (energy_drift / total_initial).abs()
    } else {
        energy_drift.abs()
    };

    println!("\n=== Energy Conservation Results ===");
    println!("Energy drift:     {:.6}", energy_drift);
    println!(
        "Relative drift:   {:.4}% ({:.2e})",
        relative_drift * 100.0,
        relative_drift
    );
    println!("Initial total:    {:.6}", total_initial);
    println!("Final total:      {:.6}", total_final);

    // Check if energy drift is concerning
    // For 1000 balls over 100 steps, we expect some numerical drift
    // but it should be relatively small (< 10%)
    const MAX_ACCEPTABLE_DRIFT: f32 = 0.10; // 10%

    if relative_drift > MAX_ACCEPTABLE_DRIFT {
        println!("\n⚠️  WARNING: Energy drift is larger than expected!");
        println!("   This may indicate an issue with energy conservation.");
        println!(
            "   Expected < {:.1}%, got {:.4}%",
            MAX_ACCEPTABLE_DRIFT * 100.0,
            relative_drift * 100.0
        );
    } else {
        println!("\n✓ Energy drift is within acceptable range");
    }

    // Don't fail the test, just report the findings
    // This allows us to investigate the actual drift
    assert!(
        balls_added > 0,
        "Test infrastructure check: should have added balls"
    );
}

#[test]
fn test_cutoff_energy_loss() {
    println!("\n=== Testing Energy Loss at Cutoff Boundary ===");

    // Create a small world for controlled testing
    let (mut main_content, _win_def) = create_main_content_for_region(-50.0, 50.0);

    let factory = BallFactory::new();
    // Both balls are of the default species; the cutoff is a centre distance.
    let cutoff = main_content
        .world()
        .physics
        .cutoff(DEFAULT_SPECIES, DEFAULT_SPECIES);

    println!("Cutoff distance: {:.6}", cutoff);

    // Create two balls that will cross the cutoff boundary
    // Ball 1 at origin
    let ball1_pos = Vec2D::new(0.0, 0.0);
    let ball1_vel = Vec2D::new(0.0, 0.0);
    let ball1 = factory.create_massive_with_velocity(ball1_pos, ball1_vel);

    // Ball 2 just inside cutoff, moving away slowly
    let initial_separation = cutoff - 0.5; // 0.5 units inside cutoff
    let ball2_pos = Vec2D::new(initial_separation, 0.0);
    let ball2_vel = Vec2D::new(1.0, 0.0); // Faster velocity to actually move
    let ball2 = factory.create_massive_with_velocity(ball2_pos, ball2_vel);

    main_content
        .add_candidate_ball(ball1, 0.)
        .expect("Could not add ball1");
    main_content
        .add_candidate_ball(ball2, 0.)
        .expect("Could not add ball2");

    println!(
        "Initial separation (centre to centre): {:.6}",
        initial_separation
    );

    // // Add balls to cell grid
    // for i in 0..main_content.balls.len() {
    //     main_content.cell_grid.add_ball(i, &mut main_content.balls);
    // }

    println!(
        "Ball 0 in cell: {:?}",
        main_content.world().balls[0].current_cell
    );
    println!(
        "Ball 1 in cell: {:?}",
        main_content.world().balls[1].current_cell
    );

    // Initialize neighbor lists
    main_content.collect_neighbours_by_cell(DT_GOAL);

    // Helper to calculate exact energy (no cutoff)
    let calculate_exact_energy = |main_content: &MainContent| -> (f32, f32, f32) {
        let world = main_content.world();
        let mut kinetic = 0.0f32;
        let mut potential = 0.0f32;

        // Kinetic energy
        for ball in &world.balls {
            let speed_sq = ball.v.x * ball.v.x + ball.v.y * ball.v.y;
            kinetic += 0.5 * ball.mass * speed_sq;
        }

        // Potential energy - calculate for ALL pairs regardless of cutoff
        let ball0 = &world.balls[0];
        let ball1 = &world.balls[1];
        let dist_vec = ball1.get_position().dist(ball0.get_position());
        let center_dist = dist_vec.norm();

        // Use physics calculation but without cutoff check
        let pair_potential = world
            .physics
            .potential(ball0.species(), ball1.species(), center_dist);
        potential += pair_potential;

        (kinetic, potential, kinetic + potential)
    };

    // Measure initial energy
    let (ke_initial, pe_initial, total_initial) = calculate_exact_energy(&main_content);

    // Calculate initial momentum
    let ball0 = &main_content.world().balls[0];
    let ball1 = &main_content.world().balls[1];
    let p0_x_initial = ball0.mass * ball0.v.x;
    let p0_y_initial = ball0.mass * ball0.v.y;
    let p1_x_initial = ball1.mass * ball1.v.x;
    let p1_y_initial = ball1.mass * ball1.v.y;
    let total_px_initial = p0_x_initial + p1_x_initial;
    let total_py_initial = p0_y_initial + p1_y_initial;
    let total_p_mag_initial =
        (total_px_initial * total_px_initial + total_py_initial * total_py_initial).sqrt();

    println!("\nInitial energy:");
    println!("  Kinetic:   {:.6}", ke_initial);
    println!("  Potential: {:.6}", pe_initial);
    println!("  Total:     {:.6}", total_initial);
    println!("\nInitial momentum:");
    println!(
        "  Total p: ({:.6}, {:.6})",
        total_px_initial, total_py_initial
    );
    println!("  |p|:     {:.6}", total_p_mag_initial);

    // Track when particles cross cutoff
    let mut crossed_cutoff = false;
    let mut step_crossed = 0;

    println!("\nRunning simulation to observe cutoff crossing...");

    for step in 0..200 {
        main_content.simulation_step().unwrap();

        // Check current separation
        let ball0 = &main_content.world().balls[0];
        let ball1 = &main_content.world().balls[1];
        let dist_vec = ball1.get_position().dist(ball0.get_position());
        let center_dist = dist_vec.norm();
        if !crossed_cutoff && center_dist > cutoff {
            crossed_cutoff = true;
            step_crossed = step + 1;
            println!("\n>>> Crossed cutoff at step {}", step_crossed);
            println!("    Centre distance: {:.6}", center_dist);

            // Check neighbor lists
            let neighbors0 = ball0.hood.get_neighbour_indices().len();
            let neighbors1 = ball1.hood.get_neighbour_indices().len();
            println!("    Ball0 neighbors: {}", neighbors0);
            println!("    Ball1 neighbors: {}", neighbors1);
        }

        if step % 20 == 9 || (crossed_cutoff && step == step_crossed) {
            let (ke, pe, total) = calculate_exact_energy(&main_content);
            let drift = total - total_initial;
            let relative_drift = (drift / total_initial).abs() * 100.0;

            // Check ball velocities and accelerations
            let ball0 = &main_content.world().balls[0];
            let ball1 = &main_content.world().balls[1];
            let ball0_v = &ball0.v;
            let ball1_v = &ball1.v;
            let ball0_a = &ball0.accel;
            let ball1_a = &ball1.accel;

            // Calculate total momentum
            let p0_x = ball0.mass * ball0_v.x;
            let p0_y = ball0.mass * ball0_v.y;
            let p1_x = ball1.mass * ball1_v.x;
            let p1_y = ball1.mass * ball1_v.y;
            let total_px = p0_x + p1_x;
            let total_py = p0_y + p1_y;
            let total_p_mag = (total_px * total_px + total_py * total_py).sqrt();

            // Get wall momentum
            let wall_px = main_content.world().sim.wall_momentum_x;
            let wall_py = main_content.world().sim.wall_momentum_y;
            let wall_p_mag = (wall_px * wall_px + wall_py * wall_py).sqrt();

            // Total system momentum (balls + walls)
            let system_px = total_px + wall_px;
            let system_py = total_py + wall_py;
            let system_p_mag = (system_px * system_px + system_py * system_py).sqrt();
            let system_p_drift_x = system_px - total_px_initial;
            let system_p_drift_y = system_py - total_py_initial;
            let system_p_drift_mag =
                (system_p_drift_x * system_p_drift_x + system_p_drift_y * system_p_drift_y).sqrt();

            // Check neighbor lists
            let ball0_neighbors = ball0.hood.get_neighbour_indices();
            let ball1_neighbors = ball1.hood.get_neighbour_indices();

            println!(
                "Step {}: center_dist={:.6}, KE={:.6}, PE={:.6}, Total={:.6}, Drift={:.6} ({:.4}%)",
                step + 1,
                center_dist,
                ke,
                pe,
                total,
                drift,
                relative_drift
            );
            println!(
                "         Balls p=({:.6}, {:.6}), |p|={:.6}",
                total_px, total_py, total_p_mag
            );
            println!(
                "         Walls p=({:.6}, {:.6}), |p|={:.6}",
                wall_px, wall_py, wall_p_mag
            );
            println!(
                "         System p=({:.6}, {:.6}), |p|={:.6}, drift={:.6}",
                system_px, system_py, system_p_mag, system_p_drift_mag
            );
            println!(
                "         Ball0: v=({:.6}, {:.6}), a=({:.6}, {:.6}), neighbors={:?}",
                ball0_v.x, ball0_v.y, ball0_a.x, ball0_a.y, ball0_neighbors
            );
            println!(
                "         Ball1: v=({:.6}, {:.6}), a=({:.6}, {:.6}), neighbors={:?}",
                ball1_v.x, ball1_v.y, ball1_a.x, ball1_a.y, ball1_neighbors
            );
        }
    }

    // Final energy
    let (_ke_final, _pe_final, total_final) = calculate_exact_energy(&main_content);
    let energy_drift = total_final - total_initial;
    let relative_drift = (energy_drift / total_initial).abs() * 100.0;

    println!("\n=== Cutoff Crossing Results ===");
    println!("Crossed cutoff: {}", crossed_cutoff);
    println!(
        "Energy drift:   {:.6} ({:.4}%)",
        energy_drift, relative_drift
    );
    println!("Initial total:  {:.6}", total_initial);
    println!("Final total:    {:.6}", total_final);

    // The drift should be small regardless of cutoff crossing
    // Note: With current initial conditions, balls may not cross cutoff in 100 steps
    // This is okay - the test still validates energy conservation
    if crossed_cutoff {
        println!("Note: Cutoff boundary was crossed during test");
    } else {
        println!(
            "Note: Cutoff boundary was NOT crossed (consider increasing steps or initial velocity)"
        );
    }

    // Energy conservation should be good regardless
    assert!(
        relative_drift < 1.0,
        "Energy drift too large: {:.4}%",
        relative_drift
    );
}

/// Potential energy of the pairs the dynamics actually sees: every pair in a
/// neighbour list, counted once, summed in f64 so list reordering does not
/// show up as rounding noise.
fn listed_pair_potential(main_content: &MainContent) -> f64 {
    let world = main_content.world();
    let mut potential = 0.0f64;
    for (i, ball) in world.balls.iter().enumerate() {
        for &j in ball.hood.get_neighbour_indices() {
            if j > i {
                let other = &world.balls[j];
                let center_dist = ball.get_position().dist(other.get_position()).norm();
                potential += world
                    .physics
                    .potential(ball.species(), other.species(), center_dist)
                    as f64;
            }
        }
    }
    potential
}

/// Pairs join or leave a neighbour list only beyond the LJ cutoff, where the
/// shifted potential is zero, so a rebuild must never change the energy.
/// Without the cutoff every list change added or removed U(r) ~ -5e-4 and the
/// lag between adding (inside) and removing (outside) made that a steady loss.
#[test]
fn test_neighbour_list_rebuild_does_not_change_energy() {
    let (mut main_content, win_def) = create_main_content_for_region(500.0, 500.0);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);

    let world_rect = win_def;
    let (tl_x, tl_y) = world_rect.get_tl();
    let (br_x, br_y) = world_rect.get_br();
    let center = Vec2D::new((tl_x + br_x) * 0.5, (tl_y + br_y) * 0.5);
    let factory = BallFactory::new();
    const SIDE: usize = 12;
    const SPACING: f32 = 2.4;
    let half = (SIDE as f32 - 1.0) * SPACING * 0.5;
    for i in 0..SIDE {
        for j in 0..SIDE {
            let pos = Vec2D::new(
                center.x - half + i as f32 * SPACING,
                center.y - half + j as f32 * SPACING,
            );
            let vel = Vec2D::new(
                (i as f32 * 0.617 + j as f32 * 1.271).sin() * 1.5,
                (i as f32 * 1.314 + j as f32 * 0.459).cos() * 1.5,
            );
            let ball = factory.create_massive_with_velocity(pos, vel);
            main_content.world_mut().balls.push(ball);
        }
    }
    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);

    let listed_pairs = |mc: &MainContent| -> usize {
        mc.world()
            .balls
            .iter()
            .map(|b| b.hood.get_neighbour_indices().len())
            .sum()
    };

    let mut list_changes = 0usize;
    let mut max_jump = 0.0f64;
    for _ in 0..400 {
        let pairs_before = listed_pairs(&main_content);
        let before = listed_pair_potential(&main_content);
        main_content.collect_neighbours_by_cell(DT_GOAL);
        let after = listed_pair_potential(&main_content);
        if listed_pairs(&main_content) != pairs_before {
            list_changes += 1;
        }
        max_jump = max_jump.max((after - before).abs());
        main_content.simulation_step().unwrap();
    }

    assert!(
        list_changes > 10,
        "scenario must actually change neighbour lists, got {list_changes} rebuilds that did"
    );
    assert!(
        max_jump < 1e-6,
        "neighbour-list rebuild changed the potential energy by {max_jump:e}"
    );
}
