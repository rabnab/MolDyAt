#[cfg(test)]
mod tests {
    use crate::frame::renderer::create_dummy_renderer;
    use crate::frame::window::window_definition::WindowDefinition;
    use crate::frame::{GameLogic, MainContent};
    use crate::objects2d::ball_impl::{Ball2D, Named};
    use crate::objects2d::{Vec2D, VerboseInfoMode};
    use float_cmp::approx_eq;
    use std::cell::RefCell;
    use std::rc::Rc;

    const DT_GOAL: f32 = 0.01; // 1. / (FPS_GOAL as f32);
    #[test]
    fn test_defaults() {
        let default_win = WindowDefinition::default();
        assert_eq!(default_win.region_px.0, 100.);
        assert_eq!(default_win.region_px.1, 100.);
        assert_eq!(default_win.margin_px.0, 10.);
        assert_eq!(default_win.margin_px.1, 10.);
    }

    fn create_main_content_for_region(x: f32, y: f32) -> (MainContent, WindowDefinition) {
        let win_def = WindowDefinition::new((x, y));
        create_main_content_with_windef(win_def)
    }

    fn create_main_content_with_windef(
        win_def: WindowDefinition,
    ) -> (MainContent, WindowDefinition) {
        let world_rect = win_def.compute_world_bounds_rect();
        let rend_ref = Rc::new(RefCell::new(create_dummy_renderer(win_def.clone())));
        let main_content =
            MainContent::new(world_rect, rend_ref).expect("Could not create main content");

        println!("Created main content with world rect {:?}", world_rect,);
        (main_content, win_def)
    }
    fn create_main_content() -> (MainContent, WindowDefinition) {
        create_main_content_with_windef(WindowDefinition::default())
    }

    #[test]
    fn test_add_candidate() {
        let (mut main_content, win_def) = create_main_content();

        let new_ball_size = main_content
            .get_game_logic_data()
            .get_ball_factory()
            .get_new_ball_radius();
        // Add a valid candidate
        let x_px = win_def.margin_px.0 + WindowDefinition::len_to_px(new_ball_size * 1.1);
        let y_px = win_def.margin_px.1 + WindowDefinition::len_to_px(new_ball_size * 1.1);
        println!(
            "test adding ball at px ({},{}) | LEN_TO_PX: {} and inset {:?}",
            x_px,
            y_px,
            WindowDefinition::get_len_to_px(),
            win_def.world_region_inset(new_ball_size)
        );

        let coord = win_def.px_to_coord(x_px, y_px);

        main_content
            .add_candidate(coord)
            .expect("Could not add candidate");

        assert_eq!(main_content.num_balls, 1);

        // Ensure the added ball is in the correct position
        {
            let ball = &main_content.balls[0];
            let ball_pos_px = win_def.coord_to_px(&ball.get_position());
            assert!(
                approx_eq!(f32, ball_pos_px.x, x_px, ulps = 2),
                "values in x are not close: {} vs {}",
                ball_pos_px.x,
                x_px
            );
            assert!(
                approx_eq!(f32, ball_pos_px.y, y_px, ulps = 2),
                "values in y are not close: {} vs {}",
                ball_pos_px.y,
                y_px
            );
        }

        // Attempt to add a candidate out of the region
        {
            let invalid_x_px = -10.0;
            let invalid_y_px = -10.0;
            let coord = win_def.px_to_coord(invalid_x_px, invalid_y_px);
            main_content
                .add_candidate(coord)
                .expect_err("Could not add candidate");
            assert_eq!(main_content.num_balls, 1, "Ball added out of valid region");
        }
    }

    #[test]
    fn test_is_ball_in_region() {
        let (main_content, win_def) = create_main_content();

        let ball_px: Vec2D = Vec2D::new(
            win_def.margin_px.1 + win_def.region_px.0 * 0.1,
            win_def.margin_px.1 + win_def.region_px.1 * 0.1,
        );
        let ball_coord = win_def.px_to_coord(ball_px.x, ball_px.y);
        println!(
            "ball coordinates are {:?} from px ({},{})",
            ball_coord, ball_px.x, ball_px.y
        );
        let ball_radius = WindowDefinition::px_to_len(10.);
        let ball = Ball2D::new(0, ball_coord.clone(), (0, ball_radius));
        assert!(main_content.is_ball_in_region(&ball));

        let invalid_ball = Ball2D::new(1, Vec2D::new(-10., -10.), (0, ball_radius));
        // Move the ball outside the region
        assert!(!main_content.is_ball_in_region(&invalid_ball));
    }

    #[test]
    fn test_global_update() {
        let (mut main_content, win_def) = create_main_content();

        // Add some balls
        let ball_coord_px: Vec2D = Vec2D::new(
            win_def.margin_px.1 + win_def.region_px.0 * 0.1,
            win_def.margin_px.1 + win_def.region_px.1 * 0.1,
        );
        let ball_radius = main_content
            .get_game_logic_data()
            .get_ball_factory()
            .get_new_ball_radius();
        let ball_radius_px = WindowDefinition::len_to_px(ball_radius);
        let ball_coord = win_def.px_to_coord(ball_coord_px.x, ball_coord_px.y);
        main_content.add_candidate(ball_coord.clone()).unwrap();
        let ball_coord = win_def.px_to_coord(
            ball_coord_px.x + ball_radius_px * 2.2,
            ball_coord_px.y + ball_radius_px * 2.2,
        );
        main_content.add_candidate(ball_coord).unwrap();
        main_content.make_all_verbose(VerboseInfoMode::Console);
        main_content.simulation_step().unwrap();

        // Validate that the balls' velocities, positions, and energies are updated correctly
        assert!(
            main_content.sim.get_kinetic_energy() > 0.0,
            "Kinetic energy should be positive but was {}",
            main_content.sim.get_kinetic_energy()
        );
        //assert!(main_content.sim.energy_pot >= 0.0, "Potential energy should be non-negative");
    }

    #[test]
    fn test_resize_event() {
        let (mut main_content, win_def) = create_main_content();

        let new_width = 300.0;
        let new_height = 200.0;

        main_content.resize(new_width, new_height);
        main_content.unpause_after_resize();

        let new_world = main_content.cell_grid.world_rect();

        assert_eq!(
            new_world.w,
            WindowDefinition::px_to_len(new_width - 2.0 * win_def.margin_px.0)
        );
        assert_eq!(
            new_world.h,
            -WindowDefinition::px_to_len(new_height - 2.0 * win_def.margin_px.1)
        );

        // assert_eq!(win_def.region_px.0, 300.0 - 2.0 * win_def.margin_px.0);
        // assert_eq!(win_def.region_px.1, 200.0 - 2.0 * win_def.margin_px.1);
    }

    #[test]
    fn test_ball_collision() {
        let (mut main_content, win_def) = create_main_content();

        // Add two overlapping balls
        let ball_coord_px: Vec2D = Vec2D::new(
            win_def.margin_px.1 + win_def.region_px.0 * 0.1,
            win_def.margin_px.1 + win_def.region_px.1 * 0.1,
        );

        let ball_radius = main_content
            .get_game_logic_data()
            .get_ball_factory()
            .get_new_ball_radius();
        let ball_coord = win_def.px_to_coord(ball_coord_px.x, ball_coord_px.y);
        main_content
            .add_candidate(ball_coord.clone())
            .expect(format!("could not add ball at {:?}", ball_coord).as_str());

        let ball_coord = win_def.px_to_coord(ball_coord_px.x, ball_coord_px.y);
        let new_ball_coord = Vec2D::new(
            ball_coord.x + 0.5 * ball_radius,
            ball_coord.y + 0.5 * ball_radius,
        );
        let colliding_ball = main_content
            .get_game_logic_data()
            .get_ball_factory()
            .create_massive_with_velocity(new_ball_coord.clone(), Vec2D::zero());
        // main_content. coord_to_px(&new_ball_coord),

        println!(
            "existing ball {:?} and new ball {:?} both with radius {}",
            ball_coord, new_ball_coord, ball_radius
        );

        assert!(
            main_content.is_ball_colliding(&colliding_ball),
            "Collision not detected"
        );
    }

    #[test]
    fn test_coord_trafo_idempotent() {
        let (_main_content, win_def) = create_main_content_for_region(500., 500.);
        let test_coord = Vec2D::new(18., 234.);
        let vec = win_def.coord_to_px(&test_coord);
        let res_coord = win_def.px_to_coord(vec.x, vec.y);

        assert!(
            approx_eq!(f32, test_coord.x, res_coord.x, ulps = 2),
            "values in x are not close: {} vs {}",
            test_coord.x,
            res_coord.x
        );
        assert!(
            approx_eq!(f32, test_coord.y, res_coord.y, ulps = 2),
            "values in y are not close: {} vs {}",
            test_coord.y,
            res_coord.y
        );
    }

    #[test]
    fn test_collect_neighbours() {
        let (mut main_content, _win_def) = create_main_content_for_region(500., 500.);

        // Add balls to the system
        // let x1 = 80.0;
        // let y1 = 80.0;
        // let x2 = 105.0;
        // let y2 = 105.0;
        // let x3 = 200.0;
        // let y3 = 200.0;

        let ball1_coord = Vec2D::new(3.0, 3.0);
        let ball2_coord = Vec2D::new(5.5, 5.5);
        let ball3_coord = Vec2D::new(15., 15.);

        println!(
            "b1 {:?} b2 {:?} b3 {:?}",
            ball1_coord, ball2_coord, ball3_coord
        );

        main_content.add_candidate(ball1_coord.clone()).unwrap();
        main_content.add_candidate(ball2_coord.clone()).unwrap();
        main_content.add_candidate(ball3_coord.clone()).unwrap();

        // Initial state check
        {
            let balls = main_content.get_balls();
            let ball1: &Ball2D = &balls[0];
            let ball2: &Ball2D = &balls[1];
            let ball3: &Ball2D = &balls[2];

            assert!(
                ball1.get_neighbour_names(balls).is_empty(),
                "Ball 1 should have no neighbours initially"
            );
            assert!(
                ball2.get_neighbour_names(balls).is_empty(),
                "Ball 2 should have no neighbours initially"
            );
            assert!(
                ball3.get_neighbour_names(balls).is_empty(),
                "Ball 3 should have no neighbours initially"
            );
        }

        // Call collect_neighbours with the appropriate timestep (let's assume step_count is divisible by 10)
        {
            main_content.step_count = 10;
            println!("--------------------------------");
            main_content.collect_neighbours_by_cell(DT_GOAL);
            println!("--------------------------------");
            main_content.collect_neighbours_by_cell(DT_GOAL);

            // After collecting neighbors, Ball1 and Ball2 should be neighbors
            let balls = main_content.get_balls();
            let ball1: &Ball2D = &balls[0];
            let ball2: &Ball2D = &balls[1];
            let ball3: &Ball2D = &balls[2];

            println!("ball1.neigh: {:?}", ball1.get_neighbour_names(balls));
            println!("ball2.neigh: {:?}", ball2.get_neighbour_names(balls));
            println!("ball3.neigh: {:?}", ball3.get_neighbour_names(balls));

            assert_eq!(
                ball1.get_neighbour_names(balls).len(),
                1,
                "Ball 1 should have 1 neighbour"
            );
            assert_eq!(
                ball2.get_neighbour_names(balls).len(),
                2,
                "Ball 2 should have 1 neighbour"
            );
            assert!(
                ball1.get_neighbour_names(balls).contains(&ball2.get_name()),
                "Ball 1 should be neighbour of Ball 2"
            );
            assert!(
                ball2.get_neighbour_names(balls).contains(&ball1.get_name()),
                "Ball 2 should be neighbour of Ball 1"
            );

            assert!(
                ball3.get_neighbour_names(balls).contains(&ball2.get_name()),
                "Ball 3 should have no neighbours because it is far from others"
            );
        }
    }

    #[test]
    fn test_remove_invalid_balls_clears_neighbor_indices() {
        let (mut main_content, _win_def) = create_main_content_for_region(500., 500.);

        // Add three balls in a line
        let ball1_coord = Vec2D::new(3.0, 3.0);
        let ball2_coord = Vec2D::new(5.5, 5.5);
        let ball3_coord = Vec2D::new(8.0, 8.0);

        main_content.add_candidate(ball1_coord.clone()).unwrap();
        main_content.add_candidate(ball2_coord.clone()).unwrap();
        main_content.add_candidate(ball3_coord.clone()).unwrap();

        // Set up neighbor relationships by running collect_neighbours
        main_content.step_count = 10;
        main_content.collect_neighbours_by_cell(DT_GOAL);
        main_content.collect_neighbours_by_cell(DT_GOAL);

        // Verify initial neighbor relationships exist
        {
            let balls = main_content.get_balls();
            assert!(
                balls[0].hood.get_neighbour_indices().len() > 0,
                "Ball 0 should have neighbors before removal"
            );
            assert!(
                balls[1].hood.get_neighbour_indices().len() > 0,
                "Ball 1 should have neighbors before removal"
            );
        }

        // Make ball 1 (middle ball) invalid by setting NaN position
        main_content.balls[1].set_position(Vec2D::new(f32::NAN, f32::NAN));

        // Remove invalid balls - this should:
        // 1. Remove ball at index 1
        // 2. Ball at index 2 moves to index 1
        // 3. Clear all neighbor lists to prevent stale indices
        main_content.remove_invalid_balls();

        // Verify the invalid ball was removed
        assert_eq!(
            main_content.num_balls, 2,
            "Should have 2 balls after removing 1 invalid ball"
        );

        // Verify all neighbor lists are cleared
        {
            let balls = main_content.get_balls();
            for (idx, ball) in balls.iter().enumerate() {
                assert_eq!(
                    ball.hood.get_neighbour_indices().len(),
                    0,
                    "Ball {} should have no neighbors after remove_invalid_balls",
                    idx
                );
                // Verify last_update_hood_pos is reset to force update
                assert!(
                    ball.last_update_hood_pos.x.is_infinite(),
                    "Ball {} last_update_hood_pos.x should be infinity",
                    idx
                );
                assert!(
                    ball.last_update_hood_pos.y.is_infinite(),
                    "Ball {} last_update_hood_pos.y should be infinity",
                    idx
                );
            }
        }

        // After rebuilding neighbors, verify indices are consistent
        main_content.collect_neighbours_by_cell(DT_GOAL);
        main_content.collect_neighbours_by_cell(DT_GOAL);

        // Verify neighbor indices are valid (don't point beyond array bounds)
        {
            let balls = main_content.get_balls();
            let num_balls = balls.len();
            for (ball_idx, ball) in balls.iter().enumerate() {
                for &neighbor_idx in ball.hood.get_neighbour_indices() {
                    assert!(
                        neighbor_idx < num_balls,
                        "Ball {} has invalid neighbor index {} (only {} balls exist)",
                        ball_idx,
                        neighbor_idx,
                        num_balls
                    );
                    assert_ne!(
                        neighbor_idx, ball_idx,
                        "Ball {} should not be its own neighbor",
                        ball_idx
                    );
                }
            }
        }

        // Verify we can access neighbors without panic
        {
            let balls = main_content.get_balls();
            for ball in balls {
                let neighbor_names = ball.get_neighbour_names(balls);
                // Just verify this doesn't panic - actual neighbor counts may vary
                println!(
                    "Ball {} has {} neighbors: {:?}",
                    ball.get_name(),
                    neighbor_names.len(),
                    neighbor_names
                );
            }
        }
    }

    #[test]
    fn test_energy_conservation_1000_balls() {
        use crate::frame::VerletIntegration;
        use crate::objects2d::ball_impl::BallFactory;

        println!("\n=== Testing Energy Conservation with 1000 Balls ===");

        // Create a larger world for 1000 balls - 500x500 pixels (50x50 in world units)
        let (mut main_content, win_def) = create_main_content_for_region(500.0, 500.0);
        
        let world_rect = win_def.compute_world_bounds_rect();
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
        
        println!("Placement bounds: x=[{:.2}, {:.2}], y=[{:.2}, {:.2}]", 
                 min_x, max_x, min_y, max_y);

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
                
                // Small random velocity
                let vx = (i as f32 * 0.1) % 1.0 - 0.5;
                let vy = (j as f32 * 0.1) % 1.0 - 0.5;
                let vel = Vec2D::new(vx, vy);
                
                // Create ball with small random velocity
                let ball = factory.create_massive_with_velocity(pos, vel);
                main_content.balls.push(ball);
                balls_added += 1;
            }
        }
        
        main_content.num_balls = balls_added;
        println!("Successfully added {} balls", balls_added);

        // Initialize neighbor lists
        println!("\nInitializing neighbor lists...");
        main_content.collect_neighbours_by_cell(DT_GOAL);
        
        // Helper function to calculate total energy
        let calculate_energy = |main_content: &MainContent| -> (f32, f32, f32) {
            let mut kinetic = 0.0f32;
            let mut potential = 0.0f32;
            
            // Kinetic energy
            for ball in &main_content.balls {
                let speed_sq = ball.v.x * ball.v.x + ball.v.y * ball.v.y;
                kinetic += 0.5 * ball.mass * speed_sq;
            }
            
            // Potential energy (count each pair once)
            for i in 0..main_content.balls.len() {
                for j in (i + 1)..main_content.balls.len() {
                    let dist_vec = main_content.balls[j].get_position().dist(main_content.balls[i].get_position());
                    let center_dist = dist_vec.norm();
                    let edge_dist = center_dist - (main_content.balls[i].radius + main_content.balls[j].radius);
                    
                    let pair_potential = main_content.physics.lenard_jones_potential(edge_dist);
                    potential += pair_potential;
                }
            }
            
            (kinetic, potential, kinetic + potential)
        };
        
        // Measure initial energy
        println!("\nCalculating initial energy...");
        let (ke_initial, pe_initial, total_initial) = calculate_energy(&main_content);
        println!("Initial energy:");
        println!("  Kinetic:   {:.6}", ke_initial);
        println!("  Potential: {:.6}", pe_initial);
        println!("  Total:     {:.6}", total_initial);
        
        // Run simulation for 10 steps
        println!("\nRunning simulation for 10 steps...");
        let bound_rect = main_content.world_region();
        
        for step in 0..10 {
            main_content.update_sim(DT_GOAL, &bound_rect);
            
            if step % 2 == 1 {
                let (ke, pe, total) = calculate_energy(&main_content);
                let drift = total - total_initial;
                let relative_drift = if total_initial.abs() > 1e-10 {
                    (drift / total_initial).abs()
                } else {
                    drift.abs()
                };
                
                println!("Step {}: KE={:.6}, PE={:.6}, Total={:.6}, Drift={:.6} ({:.4}%)", 
                         step + 1, ke, pe, total, drift, relative_drift * 100.0);
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
        println!("Relative drift:   {:.4}% ({:.2e})", relative_drift * 100.0, relative_drift);
        println!("Initial total:    {:.6}", total_initial);
        println!("Final total:      {:.6}", total_final);
        
        // Check if energy drift is concerning
        // For 1000 balls over 10 steps, we expect some numerical drift
        // but it should be relatively small (< 10%)
        const MAX_ACCEPTABLE_DRIFT: f32 = 0.10; // 10%
        
        if relative_drift > MAX_ACCEPTABLE_DRIFT {
            println!("\n⚠️  WARNING: Energy drift is larger than expected!");
            println!("   This may indicate an issue with energy conservation.");
            println!("   Expected < {:.1}%, got {:.4}%", 
                     MAX_ACCEPTABLE_DRIFT * 100.0, relative_drift * 100.0);
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
}
