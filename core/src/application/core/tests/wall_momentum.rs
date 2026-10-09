// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{DT_GOAL, create_main_content_for_region};
use crate::application::VerletIntegration;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::BallFactory;

#[test]
fn test_single_ball_wall_bounce_momentum() {
    // Test that a single ball bouncing off a wall conserves momentum perfectly.
    //
    // This test verifies that the split timestep integration correctly handles
    // wall bounces by:
    // 1. Integrating velocity to the collision time
    // 2. Applying elastic bounce (velocity reversal)
    // 3. Transferring momentum to wall: wall_p = 2*m*v_at_collision
    // 4. Continuing integration for the remainder of the timestep
    //
    // The speed limit is disabled (set to infinity) to test pure momentum
    // conservation without artificial damping.
    // Create a small region so ball hits wall quickly
    let (mut main_content, _) = create_main_content_for_region(100.0, 100.0);

    // Disable speed limit for momentum conservation testing
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);

    let factory = BallFactory::new();

    // Create a ball moving toward the right wall
    // Position it so it will hit the wall in a few steps
    let ball_radius = factory.get_new_ball_radius();
    // One unit left of the right wall (the region spans x = 100..200).
    let (right_x, _) = main_content.world().world_region().get_br();
    let (_, top_y) = main_content.world().world_region().get_tl();
    let initial_x = right_x - ball_radius - 1.0;
    let initial_y = top_y - 50.0;
    // Use a higher velocity now that speed limit is disabled
    let initial_vx = 5.0;
    let initial_vy = 0.0;

    let pos = Vec2D::new(initial_x, initial_y); // Near right edge of default region
    let initial_velocity = Vec2D::new(initial_vx, initial_vy); // Fast rightward velocity
    let ball = factory.create_massive_with_velocity(pos, initial_velocity);
    let ball_mass = ball.mass;

    main_content.world_mut().balls.push(ball);
    main_content.world_mut().rebuild_cell_grid();

    let bound_rect = main_content.world().world_region();

    // Verify gravity is zero
    let gravity_value = main_content.world().physics.constants.gravity;
    assert_eq!(gravity_value, 0.0, "Gravity should be 0 for this test");

    println!("\n=== Single Ball Wall Bounce Test ===");
    println!("Gravity: {:.4}", gravity_value);
    println!("Ball radius: {:.4}", ball_radius);
    let (tl_x, tl_y) = bound_rect.get_tl();
    let (br_x, br_y) = bound_rect.get_br();
    println!(
        "World bounds: x=[{:.2}, {:.2}], y=[{:.2}, {:.2}]",
        tl_x, br_x, tl_y, br_y
    );
    println!("Initial position: ({:.4}, {:.4})", initial_x, initial_y);
    println!("Initial velocity: ({:.4}, {:.4})", initial_vx, initial_vy);
    println!("Ball mass: {:.4}", ball_mass);

    let initial_ball_momentum = ball_mass * initial_vx;
    println!("Initial ball momentum: {:.6}", initial_ball_momentum);

    // Run until ball bounces off wall
    let mut bounced = false;
    let mut step = 0;
    let max_steps = 100;

    while !bounced && step < max_steps {
        main_content.update_sim(DT_GOAL, &bound_rect);
        step += 1;

        let ball = &main_content.world().balls[0];
        let v_x = ball.v.x;
        let pos_x = ball.get_position().x;

        // Debug: print position every 10 steps
        if step % 10 == 0 {
            println!(
                "Step {}: pos_x={:.4}, v_x={:.4}, accel_x={:.4}",
                step, pos_x, v_x, ball.accel.x
            );
        }

        // Check if velocity reversed (bounce happened)
        if v_x < 0.0 {
            bounced = true;
            println!("\nBounce detected at step {}", step);
            let ball_pos = ball.get_position();
            println!(
                "Ball position after bounce: ({:.4}, {:.4})",
                ball_pos.x, ball_pos.y
            );
            println!(
                "Ball velocity after bounce: ({:.4}, {:.4})",
                ball.v.x, ball.v.y
            );

            let final_ball_momentum = ball_mass * ball.v.x;
            let ball_momentum_change = final_ball_momentum - initial_ball_momentum;

            println!("\nMomentum Analysis:");
            println!("  Initial ball momentum: {:.6}", initial_ball_momentum);
            println!("  Final ball momentum:   {:.6}", final_ball_momentum);
            println!("  Ball momentum change:  {:.6}", ball_momentum_change);
            println!(
                "  Wall momentum:         {:.6}",
                main_content.world().sim.wall_momentum_x
            );
            println!(
                "  System momentum:       {:.6}",
                final_ball_momentum + main_content.world().sim.wall_momentum_x
            );

            let momentum_conservation_error = (final_ball_momentum
                + main_content.world().sim.wall_momentum_x)
                - initial_ball_momentum;
            println!(
                "  Conservation error:    {:.10}",
                momentum_conservation_error
            );

            // For a perfect elastic bounce with no other forces:
            // - Ball momentum should reverse: p_final ≈ -p_initial
            // - Wall should absorb 2*p_initial
            // - System momentum should be conserved

            let expected_wall_momentum = -ball_momentum_change;
            let wall_momentum_error =
                main_content.world().sim.wall_momentum_x - expected_wall_momentum;

            println!("\nExpected vs Actual:");
            println!("  Expected wall momentum: {:.6}", expected_wall_momentum);
            println!(
                "  Actual wall momentum:   {:.6}",
                main_content.world().sim.wall_momentum_x
            );
            println!("  Wall momentum error:    {:.10}", wall_momentum_error);

            // The system momentum should be conserved (sum should equal initial)
            assert!(
                momentum_conservation_error.abs() < 1e-4,
                "Momentum not conserved! Error: {:.10}",
                momentum_conservation_error
            );

            // Wall momentum should equal negative of ball momentum change
            assert!(
                wall_momentum_error.abs() < 1e-4,
                "Wall momentum incorrect! Error: {:.10}",
                wall_momentum_error
            );
        }
    }

    assert!(bounced, "Ball did not bounce within {} steps", max_steps);
}
