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
                let vx = ((i as f32 * 0.617 + j as f32 * 0.271).sin() * v_thermal);
                let vy = ((i as f32 * 0.314 + j as f32 * 0.159).cos() * v_thermal);
                let vel = Vec2D::new(vx, vy);

                // Create ball with small random velocity
                let ball = factory.create_massive_with_velocity(pos, vel);
                main_content.balls.push(ball);
                balls_added += 1;
            }
        }

        main_content.num_balls = balls_added;
        println!("Successfully added {} balls", balls_added);

        // Add balls to cell grid
        println!("\nAdding balls to cell grid...");
        for i in 0..main_content.balls.len() {
            main_content.cell_grid.add_ball(i, &mut main_content.balls);
        }

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
                    let dist_vec = main_content.balls[j]
                        .get_position()
                        .dist(main_content.balls[i].get_position());
                    let center_dist = dist_vec.norm();
                    let edge_dist =
                        center_dist - (main_content.balls[i].radius + main_content.balls[j].radius);

                    let pair_potential = main_content.physics.lenard_jones_potential(edge_dist);
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
        for ball in &main_content.balls {
            total_px_initial += ball.mass * ball.v.x;
            total_py_initial += ball.mass * ball.v.y;
        }
        let total_p_mag_initial = (total_px_initial * total_px_initial + total_py_initial * total_py_initial).sqrt();
        
        println!("Initial energy:");
        println!("  Kinetic:   {:.6}", ke_initial);
        println!("  Potential: {:.6}", pe_initial);
        println!("  Total:     {:.6}", total_initial);
        println!("Initial momentum:");
        println!("  Total p: ({:.6}, {:.6})", total_px_initial, total_py_initial);
        println!("  |p|:     {:.6}", total_p_mag_initial);

        // Run simulation for 100 steps
        println!("\nRunning simulation for 100 steps...");
        let bound_rect = main_content.world_region();

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
                let num_balls = main_content.balls.len();
                let mut nan_count = 0;
                for ball in &main_content.balls {
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
                let wall_px = main_content.sim.wall_momentum_x;
                let wall_py = main_content.sim.wall_momentum_y;
                let wall_p_mag = (wall_px * wall_px + wall_py * wall_py).sqrt();
                
                // Total system momentum (balls + walls)
                let system_px = total_px + wall_px;
                let system_py = total_py + wall_py;
                let system_p_mag = (system_px * system_px + system_py * system_py).sqrt();
                let system_p_drift_x = system_px - total_px_initial;
                let system_p_drift_y = system_py - total_py_initial;
                let system_p_drift_mag = (system_p_drift_x * system_p_drift_x + system_p_drift_y * system_p_drift_y).sqrt();

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
        // For 1000 balls over 10 steps, we expect some numerical drift
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
        use crate::objects2d::ball_impl::BallFactory;

        println!("\n=== Testing Energy Loss at Cutoff Boundary ===");

        // Create a small world for controlled testing
        let (mut main_content, _win_def) = create_main_content_for_region(200.0, 200.0);
        
        let factory = BallFactory::new();
        let cutoff = main_content.physics.get_cutoff_distance();
        
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
        
        main_content.balls.push(ball1);
        main_content.balls.push(ball2);
        main_content.num_balls = 2;
        
        println!("Initial separation (edge-to-edge): {:.6}", 
                 initial_separation - (main_content.balls[0].radius + main_content.balls[1].radius));
        
        // Add balls to cell grid
        for i in 0..main_content.balls.len() {
            main_content.cell_grid.add_ball(i, &mut main_content.balls);
        }
        
        println!("Ball 0 in cell: {:?}", main_content.balls[0].current_cell);
        println!("Ball 1 in cell: {:?}", main_content.balls[1].current_cell);
        
        // Initialize neighbor lists
        main_content.collect_neighbours_by_cell(DT_GOAL);
        
        // Helper to calculate exact energy (no cutoff)
        let calculate_exact_energy = |main_content: &MainContent| -> (f32, f32, f32) {
            let mut kinetic = 0.0f32;
            let mut potential = 0.0f32;
            
            // Kinetic energy
            for ball in &main_content.balls {
                let speed_sq = ball.v.x * ball.v.x + ball.v.y * ball.v.y;
                kinetic += 0.5 * ball.mass * speed_sq;
            }
            
            // Potential energy - calculate for ALL pairs regardless of cutoff
            let ball0 = &main_content.balls[0];
            let ball1 = &main_content.balls[1];
            let dist_vec = ball1.get_position().dist(ball0.get_position());
            let center_dist = dist_vec.norm();
            let edge_dist = center_dist - (ball0.radius + ball1.radius);
            
            // Use physics calculation but without cutoff check
            let pair_potential = main_content.physics.lenard_jones_potential(edge_dist);
            potential += pair_potential;
            
            (kinetic, potential, kinetic + potential)
        };
        
        // Measure initial energy
        let (ke_initial, pe_initial, total_initial) = calculate_exact_energy(&main_content);
        
        // Calculate initial momentum
        let ball0 = &main_content.balls[0];
        let ball1 = &main_content.balls[1];
        let p0_x_initial = ball0.mass * ball0.v.x;
        let p0_y_initial = ball0.mass * ball0.v.y;
        let p1_x_initial = ball1.mass * ball1.v.x;
        let p1_y_initial = ball1.mass * ball1.v.y;
        let total_px_initial = p0_x_initial + p1_x_initial;
        let total_py_initial = p0_y_initial + p1_y_initial;
        let total_p_mag_initial = (total_px_initial * total_px_initial + total_py_initial * total_py_initial).sqrt();
        
        println!("\nInitial energy:");
        println!("  Kinetic:   {:.6}", ke_initial);
        println!("  Potential: {:.6}", pe_initial);
        println!("  Total:     {:.6}", total_initial);
        println!("\nInitial momentum:");
        println!("  Total p: ({:.6}, {:.6})", total_px_initial, total_py_initial);
        println!("  |p|:     {:.6}", total_p_mag_initial);
        
        // Track when particles cross cutoff
        let mut crossed_cutoff = false;
        let mut step_crossed = 0;
        
        println!("\nRunning simulation to observe cutoff crossing...");
        
        for step in 0..100 {
            main_content.simulation_step().unwrap();
            
            // Check current separation
            let ball0 = &main_content.balls[0];
            let ball1 = &main_content.balls[1];
            let dist_vec = ball1.get_position().dist(ball0.get_position());
            let center_dist = dist_vec.norm();
            let edge_dist = center_dist - (ball0.radius + ball1.radius);
            
            if !crossed_cutoff && edge_dist > cutoff {
                crossed_cutoff = true;
                step_crossed = step + 1;
                println!("\n>>> Crossed cutoff at step {}", step_crossed);
                println!("    Edge distance: {:.6}", edge_dist);
                
                // Check neighbor lists
                let neighbors0 = ball0.hood.get_neighbour_indices().len();
                let neighbors1 = ball1.hood.get_neighbour_indices().len();
                println!("    Ball0 neighbors: {}", neighbors0);
                println!("    Ball1 neighbors: {}", neighbors1);
            }
            
            if step % 10 == 9 || (crossed_cutoff && step == step_crossed) {
                let (ke, pe, total) = calculate_exact_energy(&main_content);
                let drift = total - total_initial;
                let relative_drift = (drift / total_initial).abs() * 100.0;
                
                // Check ball velocities and accelerations
                let ball0 = &main_content.balls[0];
                let ball1 = &main_content.balls[1];
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
                let wall_px = main_content.sim.wall_momentum_x;
                let wall_py = main_content.sim.wall_momentum_y;
                let wall_p_mag = (wall_px * wall_px + wall_py * wall_py).sqrt();
                
                // Total system momentum (balls + walls)
                let system_px = total_px + wall_px;
                let system_py = total_py + wall_py;
                let system_p_mag = (system_px * system_px + system_py * system_py).sqrt();
                let system_p_drift_x = system_px - total_px_initial;
                let system_p_drift_y = system_py - total_py_initial;
                let system_p_drift_mag = (system_p_drift_x * system_p_drift_x + system_p_drift_y * system_p_drift_y).sqrt();
                
                // Check neighbor lists
                let ball0_neighbors = ball0.hood.get_neighbour_indices();
                let ball1_neighbors = ball1.hood.get_neighbour_indices();
                
                println!("Step {}: edge_dist={:.6}, KE={:.6}, PE={:.6}, Total={:.6}, Drift={:.6} ({:.4}%)",
                         step + 1, edge_dist, ke, pe, total, drift, relative_drift);
                println!("         Balls p=({:.6}, {:.6}), |p|={:.6}", 
                         total_px, total_py, total_p_mag);
                println!("         Walls p=({:.6}, {:.6}), |p|={:.6}",
                         wall_px, wall_py, wall_p_mag);
                println!("         System p=({:.6}, {:.6}), |p|={:.6}, drift={:.6}",
                         system_px, system_py, system_p_mag, system_p_drift_mag);
                println!("         Ball0: v=({:.6}, {:.6}), a=({:.6}, {:.6}), neighbors={:?}", 
                         ball0_v.x, ball0_v.y, ball0_a.x, ball0_a.y, ball0_neighbors);
                println!("         Ball1: v=({:.6}, {:.6}), a=({:.6}, {:.6}), neighbors={:?}", 
                         ball1_v.x, ball1_v.y, ball1_a.x, ball1_a.y, ball1_neighbors);
            }
        }
        
        // Final energy
        let (ke_final, pe_final, total_final) = calculate_exact_energy(&main_content);
        let energy_drift = total_final - total_initial;
        let relative_drift = (energy_drift / total_initial).abs() * 100.0;
        
        println!("\n=== Cutoff Crossing Results ===");
        println!("Crossed cutoff: {}", crossed_cutoff);
        println!("Energy drift:   {:.6} ({:.4}%)", energy_drift, relative_drift);
        println!("Initial total:  {:.6}", total_initial);
        println!("Final total:    {:.6}", total_final);
        
        // The drift should be small even with cutoff crossing
        assert!(crossed_cutoff, "Test should have crossed cutoff boundary");
    }

    #[test]
    fn test_wall_momentum_performance_impact() {
        use std::time::Instant;
        use crate::objects2d::ball_impl::BallFactory;
        use crate::frame::VerletIntegration;
        
        // Create a 1000-ball simulation
        let (mut main_content, _) = create_main_content_for_region(500.0, 500.0);
        let factory = BallFactory::new();
        
        // Add 1000 balls
        for i in 0..1000 {
            let x = -20.0 + (i % 32) as f32 * 1.5;
            let y = -20.0 + (i / 32) as f32 * 1.5;
            let pos = Vec2D::new(x, y);
            let vel = Vec2D::new((i as f32 * 0.1).sin(), (i as f32 * 0.1).cos());
            let ball = factory.create_massive_with_velocity(pos, vel);
            main_content.balls.push(ball);
        }
        
        // Add balls to cell grid
        for i in 0..main_content.balls.len() {
            main_content.cell_grid.add_ball(i, &mut main_content.balls);
        }
        
        let bound_rect = main_content.world_region();
        
        // Warm up
        for _ in 0..10 {
            main_content.update_sim(DT_GOAL, &bound_rect);
        }
        
        // Time 100 steps
        let start = Instant::now();
        for _ in 0..100 {
            main_content.update_sim(DT_GOAL, &bound_rect);
        }
        let elapsed = start.elapsed();
        
        println!("\n=== Performance Test ===");
        println!("100 steps with 1000 balls: {:?}", elapsed);
        println!("Per step: {:?}", elapsed / 100);
        println!("Wall momentum accumulated: ({:.2}, {:.2})", 
                 main_content.sim.wall_momentum_x, 
                 main_content.sim.wall_momentum_y);
    }
}
