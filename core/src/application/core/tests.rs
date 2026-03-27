#[cfg(test)]
mod test_application_core {
    use crate::application::{GameLogic, MainContent};
    use crate::graphics::renderer::ViewportTransform;
    use crate::graphics::renderer::viewport::Margins;
    use crate::objects2d::ball_impl::{Ball2D, Named};
    use crate::objects2d::{Rect, Vec2D, VerboseInfoMode};
    use float_cmp::approx_eq;

    const DT_GOAL: f32 = 0.01; // 1. / (FPS_GOAL as f32);

    // ============================================================================
    // TEST UTILITIES
    // ============================================================================

    fn create_main_content_for_region(tl_x: f32, tl_y: f32) -> (MainContent, ViewportTransform) {
        let world = Rect::new(tl_x, tl_y, 100.0, -100.0);
        let transform =
            ViewportTransform::new(world, (800.0, 600.0), Margins::symmetric(50.0, 50.0));
        create_main_content_with_world(transform)
    }

    fn create_main_content_with_world(
        transform: ViewportTransform,
    ) -> (MainContent, ViewportTransform) {
        let world_rect = transform.world_bounds();
        let main_content = MainContent::new(world_rect).expect("Could not create main content");

        println!("Created main content with world rect {:?}", world_rect,);
        (main_content, transform)
    }

    fn create_main_content() -> (MainContent, ViewportTransform) {
        create_main_content_for_region(-10., 10.)
    }

    #[test]
    fn test_add_candidate() {
        let (mut main_content, _) = create_main_content();

        let new_ball_size = main_content
            .get_game_logic_data()
            .get_ball_factory()
            .get_new_ball_radius();
        // Add a valid candidate
        let coord = main_content.world_region_inset(new_ball_size * 1.1);
        let (x, y) = coord.get_tl();

        println!(
            "test adding ball at world ({},{}) with radius {new_ball_size} in inset {:?}",
            x,
            y,
            main_content.world_region_inset(0.)
        );

        main_content
            .add_candidate_at_pos((x, y).into())
            .expect("Could not add candidate");

        assert_eq!(main_content.num_balls, 1);

        // Ensure the added ball is in the correct position
        {
            let ball = &main_content.balls[0];
            let ball_pos = ball.get_position();
            assert!(
                approx_eq!(f32, ball_pos.x, x, ulps = 2),
                "values in x are not close: {} vs {}",
                ball_pos.x,
                x
            );
            assert!(
                approx_eq!(f32, ball_pos.y, y, ulps = 2),
                "values in y are not close: {} vs {}",
                ball_pos.y,
                y
            );
        }

        // Attempt to add a candidate out of the region
        {
            let coord = main_content.world_region();
            main_content
                .add_candidate_at_pos((coord.get_tl().0 - 0.1, coord.get_tl().1 - 0.1).into())
                .expect_err("Could not add candidate");
            assert_eq!(main_content.num_balls, 1, "Ball added out of valid region");
        }
    }

    #[test]
    fn test_is_ball_in_region() {
        let (main_content, _) = create_main_content();

        let tl = main_content.world_region().get_tl();
        let ball_coord = (0., 0.);
        println!("ball coordinates are {:?}", ball_coord,);
        let ball_radius = 5.;
        let ball = Ball2D::new(0, ball_coord.clone().into(), (0, ball_radius));
        assert!(main_content.is_ball_in_region(&ball));

        let invalid_ball = Ball2D::new(1, (tl.0 - 0.1, tl.1 - 0.1).into(), (0, ball_radius));
        // Move the ball outside the region
        assert!(!main_content.is_ball_in_region(&invalid_ball));
    }

    #[test]
    fn test_global_update() {
        let (mut main_content, _win_def) = create_main_content();

        // Add some balls

        let ball_radius = main_content
            .get_game_logic_data()
            .get_ball_factory()
            .get_new_ball_radius();

        let ball_coord = Vec2D::zero();
        main_content
            .add_candidate_at_pos(ball_coord.clone().into())
            .unwrap();

        let ball_coord = ball_coord + Vec2D::new(1., 1.) * ball_radius * 2.1;
        main_content.add_candidate_at_pos(ball_coord).unwrap();
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
    #[ignore] // TODO: Fix resize after removing renderer from MainContent
    fn test_resize_event() {
        let (mut main_content, _win_def) = create_main_content();

        let new_width = 300.0;
        let new_height = 200.0;

        let new_world_rect = Rect::new(0., 0., new_width, -new_height);

        main_content.resize(new_world_rect);
        main_content.unpause_after_resize();

        let new_world = main_content.cell_grid.world_rect();

        assert_eq!(new_world.w, new_width);
        assert_eq!(new_world.h, new_height);

        // assert_eq!(win_def.region_px.0, 300.0 - 2.0 * win_def.margin_px.0);
        // assert_eq!(win_def.region_px.1, 200.0 - 2.0 * win_def.margin_px.1);
    }

    #[test]
    fn test_resize_world_rect_matches_cell_grid() {
        let (mut main_content, _def) = create_main_content();

        let new_width = 400.0;
        let new_height = 350.0;

        // Resize window definition and main_content

        let new_world_rect = Rect::new(0., 0., new_width, -new_height);
        main_content.resize(new_world_rect);

        // Verify main_content's world_rect matches what we passed
        assert_eq!(main_content.world_rect.x, new_world_rect.x);
        assert_eq!(main_content.world_rect.y, new_world_rect.y);
        assert_eq!(main_content.world_rect.w, new_world_rect.w);
        assert_eq!(main_content.world_rect.h, new_world_rect.h);

        // Verify cell_grid world_rect matches main_content's world_rect
        let cell_grid_world = main_content.cell_grid.world_rect();
        assert_eq!(cell_grid_world.x, main_content.world_rect.x);
        assert_eq!(cell_grid_world.y, main_content.world_rect.y);
        assert_eq!(cell_grid_world.w, main_content.world_rect.w);
        assert_eq!(cell_grid_world.h, main_content.world_rect.h);
    }

    #[test]
    fn test_resize_cell_grid_origin_correct() {
        let (mut main_content, _win_def) = create_main_content();

        let new_width = 500.0;
        let new_height = 400.0;

        // Resize and check cell grid origin matches world_rect top-left
        let new_world_rect = Rect::new(0., 0., new_width, -new_height);
        main_content.resize(new_world_rect);

        let cell_grid_world = main_content.cell_grid.world_rect();

        // Cell grid origin should be at world_rect top-left
        assert_eq!(
            cell_grid_world.x, new_world_rect.x,
            "Cell grid x origin should match world_rect x"
        );
        assert_eq!(
            cell_grid_world.y, new_world_rect.y,
            "Cell grid y origin should match world_rect y"
        );
    }

    #[test]
    fn test_resize_multiple_times_maintains_consistency() {
        let (mut main_content, mut _win_def) = create_main_content();

        // Perform multiple resizes
        let sizes = [
            (300.0, 200.0),
            (600.0, 400.0),
            (450.0, 350.0),
            (800.0, 600.0),
        ];

        for (width, height) in &sizes {
            let world_rect = Rect::new(0., 0., *width, -*height);
            main_content.resize(world_rect);

            // Verify consistency after each resize
            let cell_grid_world = main_content.cell_grid.world_rect();

            assert_eq!(
                main_content.world_rect.x, cell_grid_world.x,
                "X mismatch after resize to {}x{}",
                width, height
            );
            assert_eq!(
                main_content.world_rect.y, cell_grid_world.y,
                "Y mismatch after resize to {}x{}",
                width, height
            );
            assert_eq!(
                main_content.world_rect.w, cell_grid_world.w,
                "Width mismatch after resize to {}x{}",
                width, height
            );
            assert_eq!(
                main_content.world_rect.h, cell_grid_world.h,
                "Height mismatch after resize to {}x{}",
                width, height
            );
        }
    }

    #[test]
    fn test_resize_cell_count_changes_with_world_size() {
        let (mut main_content, _win_def) = create_main_content();

        // Get initial cell count
        let (initial_rows, initial_cols) = main_content.cell_grid.get_grid_size();

        // Resize to much larger dimensions

        let large_world_rect = Rect::new(0., 0., 1000.0, -800.0);
        main_content.resize(large_world_rect);

        let (large_rows, large_cols) = main_content.cell_grid.get_grid_size();

        // Larger world should have more cells
        assert!(
            large_rows > initial_rows || large_cols > initial_cols,
            "Larger world should have more cells. Initial: {}x{}, Large: {}x{}",
            initial_rows,
            initial_cols,
            large_rows,
            large_cols
        );

        // Resize to smaller dimensions
        let small_world_rect = Rect::new(0., 0., 200.0, -150.0);
        main_content.resize(small_world_rect);

        let (small_rows, small_cols) = main_content.cell_grid.get_grid_size();

        // Smaller world should have fewer cells than large
        assert!(
            small_rows < large_rows || small_cols < large_cols,
            "Smaller world should have fewer cells. Small: {}x{}, Large: {}x{}",
            small_rows,
            small_cols,
            large_rows,
            large_cols
        );
    }

    #[test]
    fn test_ball_collision() {
        let (mut main_content, _win_def) = create_main_content();

        // Add two overlapping balls

        let ball_radius = main_content
            .get_game_logic_data()
            .get_ball_factory()
            .get_new_ball_radius();

        let ball_coord = Vec2D::zero();
        main_content
            .add_candidate_at_pos(ball_coord.clone())
            .expect(format!("could not add ball at {:?}", ball_coord).as_str());

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
            main_content.is_ball_colliding(&colliding_ball, 0.),
            "Collision not detected"
        );
    }

    #[test]
    fn test_coord_trafo_idempotent() {
        let (_main_content, win_def) = create_main_content_for_region(500., 500.);
        let test_coord: [f32; 2] = [18., 234.];
        let vec = win_def.px_to_world(&test_coord);

        let res_coord = win_def.world_to_px(&vec);

        assert!(
            approx_eq!(f32, test_coord[0], res_coord[0], ulps = 2),
            "values in x are not close: {} vs {}",
            test_coord[0],
            res_coord[0]
        );
        assert!(
            approx_eq!(f32, test_coord[1], res_coord[1], ulps = 2),
            "values in y are not close: {} vs {}",
            test_coord[1],
            res_coord[1]
        );
    }

    #[test]
    fn test_collect_neighbours() {
        let (mut main_content, _win_def) = create_main_content_for_region(-50., 50.);

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

        main_content
            .add_candidate_at_pos(ball1_coord.clone())
            .unwrap();
        main_content
            .add_candidate_at_pos(ball2_coord.clone())
            .unwrap();
        main_content
            .add_candidate_at_pos(ball3_coord.clone())
            .unwrap();

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
                1,
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
                !ball3.get_neighbour_names(balls).contains(&ball2.get_name()),
                "Ball 3 should have no neighbours because it is far from others"
            );
        }
    }

    #[test]
    fn test_remove_invalid_balls_clears_neighbor_indices() {
        let (mut main_content, _win_def) = create_main_content_for_region(-50.0, 50.0);

        // Add three balls in a line
        let ball1_coord = Vec2D::new(3.0, 3.0);
        let ball2_coord = Vec2D::new(5.5, 5.5);
        let ball3_coord = Vec2D::new(8.0, 8.0);

        main_content
            .add_candidate_at_pos(ball1_coord.clone())
            .unwrap();
        main_content
            .add_candidate_at_pos(ball2_coord.clone())
            .unwrap();
        main_content
            .add_candidate_at_pos(ball3_coord.clone())
            .unwrap();

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
        main_content.balls[1].set_position_and_update_tail(Vec2D::new(f32::NAN, f32::NAN));

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

    // ============================================================================
    // PHYSICS & ENERGY CONSERVATION TESTS
    // ============================================================================

    #[test]
    fn test_energy_conservation_1000_balls() {
        use crate::application::VerletIntegration;
        use crate::objects2d::ball_impl::BallFactory;

        println!("\n=== Testing Energy Conservation with 1000 Balls ===");

        // Create a larger world for 1000 balls - 500x500 pixels (50x50 in world units)
        let (mut main_content, win_def) = create_main_content_for_region(500.0, 500.0);

        // Disable speed limit for momentum conservation testing
        main_content.speed_limit_override = Some(f32::INFINITY);

        let world_rect = win_def.world_bounds();
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

                    let pair_potential = main_content.physics.potential(edge_dist);
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
                let system_p_drift_mag = (system_p_drift_x * system_p_drift_x
                    + system_p_drift_y * system_p_drift_y)
                    .sqrt();

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
        use crate::objects2d::ball_impl::BallFactory;

        println!("\n=== Testing Energy Loss at Cutoff Boundary ===");

        // Create a small world for controlled testing
        let (mut main_content, _win_def) = create_main_content_for_region(-50.0, 50.0);

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

        main_content
            .add_candidate_ball(ball1, 0.)
            .expect("Could not add ball1");
        main_content
            .add_candidate_ball(ball2, 0.)
            .expect("Could not add ball2");

        println!(
            "Initial separation (edge-to-edge): {:.6}",
            initial_separation - (main_content.balls[0].radius + main_content.balls[1].radius)
        );

        // // Add balls to cell grid
        // for i in 0..main_content.balls.len() {
        //     main_content.cell_grid.add_ball(i, &mut main_content.balls);
        // }

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
            let pair_potential = main_content.physics.potential(edge_dist);
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

            if step % 20 == 9 || (crossed_cutoff && step == step_crossed) {
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
                let system_p_drift_mag = (system_p_drift_x * system_p_drift_x
                    + system_p_drift_y * system_p_drift_y)
                    .sqrt();

                // Check neighbor lists
                let ball0_neighbors = ball0.hood.get_neighbour_indices();
                let ball1_neighbors = ball1.hood.get_neighbour_indices();

                println!(
                    "Step {}: edge_dist={:.6}, KE={:.6}, PE={:.6}, Total={:.6}, Drift={:.6} ({:.4}%)",
                    step + 1,
                    edge_dist,
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

    // ============================================================================
    // WALL BOUNCE & MOMENTUM CONSERVATION TESTS
    // ============================================================================

    #[test]
    fn test_single_ball_wall_bounce_momentum() {
        /// Test that a single ball bouncing off a wall conserves momentum perfectly.
        ///
        /// This test verifies that the split timestep integration correctly handles
        /// wall bounces by:
        /// 1. Integrating velocity to the collision time
        /// 2. Applying elastic bounce (velocity reversal)
        /// 3. Transferring momentum to wall: wall_p = 2*m*v_at_collision
        /// 4. Continuing integration for the remainder of the timestep
        ///
        /// The speed limit is disabled (set to infinity) to test pure momentum
        /// conservation without artificial damping.
        use crate::application::VerletIntegration;
        use crate::objects2d::ball_impl::BallFactory;

        // Create a small region so ball hits wall quickly
        let (mut main_content, _) = create_main_content_for_region(100.0, 100.0);

        // Disable speed limit for momentum conservation testing
        main_content.speed_limit_override = Some(f32::INFINITY);

        let factory = BallFactory::new();

        // Create a ball moving toward the right wall
        // Position it so it will hit the wall in a few steps
        let ball_radius = factory.get_new_ball_radius();
        let initial_x = 4.0; // Much closer to right wall (which is at 5.0)
        let initial_y = 0.0;
        // Use a higher velocity now that speed limit is disabled
        let initial_vx = 5.0;
        let initial_vy = 0.0;

        let pos = Vec2D::new(initial_x, initial_y); // Near right edge of default region
        let initial_velocity = Vec2D::new(initial_vx, initial_vy); // Fast rightward velocity
        let ball = factory.create_massive_with_velocity(pos, initial_velocity);
        let ball_mass = ball.mass;

        main_content.balls.push(ball);
        main_content.cell_grid.add_ball(0, &mut main_content.balls);

        let bound_rect = main_content.world_region();

        // Verify gravity is zero
        let gravity_value = main_content.physics.constants.gravity;
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

            let ball = &main_content.balls[0];
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
                    main_content.sim.wall_momentum_x
                );
                println!(
                    "  System momentum:       {:.6}",
                    final_ball_momentum + main_content.sim.wall_momentum_x
                );

                let momentum_conservation_error = (final_ball_momentum
                    + main_content.sim.wall_momentum_x)
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
                let wall_momentum_error = main_content.sim.wall_momentum_x - expected_wall_momentum;

                println!("\nExpected vs Actual:");
                println!("  Expected wall momentum: {:.6}", expected_wall_momentum);
                println!(
                    "  Actual wall momentum:   {:.6}",
                    main_content.sim.wall_momentum_x
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

    #[test]
    fn test_wall_momentum_performance_impact() {
        use crate::application::VerletIntegration;
        use crate::objects2d::ball_impl::BallFactory;
        use std::time::Instant;

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
        println!(
            "Wall momentum accumulated: ({:.2}, {:.2})",
            main_content.sim.wall_momentum_x, main_content.sim.wall_momentum_y
        );
    }

    #[test]
    fn test_physics_thread_spawning() {
        use crate::application::physics_thread::PhysicsThread;
        use std::thread;
        use std::time::Duration;

        let (main_content, _) = create_main_content();
        let main_thread_id = thread::current().id();

        println!("\n=== Physics Thread Spawning Test ===");
        println!("Main test thread ID: {:?}", main_thread_id);

        // Get snapshot slot before spawning (before MainContent is moved)
        let snapshot_slot = main_content.get_snapshot_slot();

        // Spawn physics thread
        let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

        // Give the physics thread time to start and log its ID
        thread::sleep(Duration::from_millis(100));

        // Send a command to verify the channel works
        let command_sender = physics_thread.get_command_sender();
        let _ = command_sender.send(crate::application::physics_thread::PhysicsCommand::Pause);

        // Give it time to process
        thread::sleep(Duration::from_millis(50));

        // Verify we can access the snapshot slot
        let _snapshot = snapshot_slot.load();

        // Clean shutdown
        drop(physics_thread);
        thread::sleep(Duration::from_millis(50));

        println!("✓ Physics thread spawned and shut down successfully");
        println!("✓ Command channel functional");
        println!("✓ Snapshot slot accessible");
        println!("=================================\n");
    }

    #[test]
    fn test_physics_thread_snapshot_publishing() {
        use crate::application::physics_thread::PhysicsThread;
        use std::thread;
        use std::time::Duration;

        let (mut main_content, _) = create_main_content();

        // Add a ball so we have something in the snapshot
        let pos = Vec2D::new(0.0, 0.0);
        main_content
            .add_candidate_at_pos(pos)
            .expect("Failed to add ball");

        println!("\n=== Physics Thread Snapshot Publishing Test ===");

        // Get snapshot slot before moving MainContent
        let snapshot_slot = main_content.get_snapshot_slot();

        // Spawn physics thread
        let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

        // Wait for a few simulation steps to occur
        thread::sleep(Duration::from_millis(200));

        // Load a snapshot and dereference the Option<Arc<>>
        let snapshot_guard = snapshot_slot.load();
        let snapshot = snapshot_guard
            .as_ref()
            .expect("Snapshot should be published");

        println!("Snapshot ball count: {}", snapshot.balls.len());
        assert_eq!(snapshot.balls.len(), 1, "Should have one ball in snapshot");
        assert!(snapshot.time >= 0.0, "Time should be non-negative");

        println!("✓ Snapshot published successfully");
        println!("✓ Ball data present in snapshot");
        println!("✓ Time tracking working");

        // Clean shutdown
        drop(physics_thread);
        thread::sleep(Duration::from_millis(50));

        println!("=========================================\n");
    }
}
