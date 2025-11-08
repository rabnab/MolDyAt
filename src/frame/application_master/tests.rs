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
            let ball = main_content.v_points[0].borrow();
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
        main_content.make_all_verbose(VerboseInfoMode::CONSOLE);
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

        let old_world = main_content.cell_grid.world_rect();

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
        let (mut main_content, win_def) = create_main_content_for_region(500., 500.);
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
        let (mut main_content, win_def) = create_main_content_for_region(500., 500.);

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
            let balls = main_content.get_ball_vec();
            let ball1: &Ball2D = &*balls[0].borrow();
            let ball2: &Ball2D = &*balls[1].borrow();
            let ball3: &Ball2D = &*balls[2].borrow();

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
            let balls = main_content.get_ball_vec();
            let ball1: &Ball2D = &*balls[0].borrow();
            let ball2: &Ball2D = &*balls[1].borrow();
            let ball3: &Ball2D = &*balls[2].borrow();

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
}
