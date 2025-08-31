#[cfg(test)]
mod tests {
    use crate::frame::window_definition::WindowDefinition;
    use crate::frame::{DrawingStuff, GameLogic, MainContent};
    use crate::objects2d::ball_impl::{Ball2D, Named};
    use crate::objects2d::{Vec2D, VerboseInfoMode};
    use float_cmp::approx_eq;

    const DT_GOAL: f32 = 0.01; // 1. / (FPS_GOAL as f32);
    #[test]
    fn test_defaults() {
        let default_win = WindowDefinition::default();
        assert_eq!(default_win.region_px.0, 100.);
        assert_eq!(default_win.region_px.1, 100.);
        assert_eq!(default_win.margin_px.0, 10.);
        assert_eq!(default_win.margin_px.1, 10.);
    }

    #[test]
    fn test_add_candidate_px() {
        let mut main_content = MainContent::default();

        // Add a valid candidate
        let x_px = main_content.win_definition.margin_px.0
            + WindowDefinition::len_to_px(main_content.new_ball.get_ball_size() * 1.1);
        let y_px = main_content.win_definition.margin_px.1
            + WindowDefinition::len_to_px(main_content.new_ball.get_ball_size() * 1.1);
        println!(
            "test adding ball at px ({},{}) | LEN_TO_PX: {}",
            x_px,
            y_px,
            WindowDefinition::get_len_to_px()
        );
        main_content.add_candidate_px(x_px, y_px).unwrap();

        assert_eq!(main_content.num_balls, 1);

        // Ensure the added ball is in the correct position
        {
            let ball = main_content.v_points[0].borrow();
            let ball_pos_px = main_content
                .win_definition
                .coord_to_px(&ball.get_position());
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
            let _ = main_content.add_candidate_px(invalid_x_px, invalid_y_px);
            assert_eq!(main_content.num_balls, 1, "Ball added out of valid region");
        }
    }

    #[test]
    fn test_is_ball_in_region() {
        let mut main_content = MainContent::default();
        let ball_px: Vec2D = Vec2D::new(30., 30.);
        let ball_coord = main_content
            .get_window_definition()
            .px_to_coord(ball_px.x, ball_px.y);
        println!(
            "ball coordinates are {:?} from px ({},{})",
            ball_coord, ball_px.x, ball_px.y
        );
        let ball_radius = WindowDefinition::px_to_len(10.);
        let ball = main_content
            .get_game_logic_data()
            .get_ball_factory()
            .create_basic(ball_coord, ball_radius);
        assert!(main_content.is_ball_in_region(&ball));

        // Move the ball outside the region
        let invalid_ball = main_content
            .get_game_logic_data()
            .get_ball_factory()
            .create_basic([-10., -10.], 5.);
        assert!(!main_content.is_ball_in_region(&invalid_ball));
    }

    #[test]
    fn test_global_update() {
        let mut main_content = MainContent::default();

        // Add some balls
        let ball_coord_px = Vec2D::new(30., 30.);
        let ball_radius_px = WindowDefinition::len_to_px(main_content.new_ball.get_ball_size());
        main_content
            .add_candidate_px(ball_coord_px.x, ball_coord_px.y)
            .unwrap();
        main_content
            .add_candidate_px(
                ball_coord_px.x + ball_radius_px * 2.2,
                ball_coord_px.y + ball_radius_px * 2.2,
            )
            .unwrap();
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
        let mut main_content = MainContent::default();
        let new_width = 300.0;
        let new_height = 200.0;

        main_content.resize(new_width, new_height).unwrap();

        assert_eq!(
            main_content.win_definition.region_px.0,
            300.0 - 2.0 * main_content.win_definition.margin_px.0
        );
        assert_eq!(
            main_content.win_definition.region_px.1,
            200.0 - 2.0 * main_content.win_definition.margin_px.1
        );
    }

    #[test]
    fn test_ball_collision() {
        let mut main_content = MainContent::default();

        // Add two overlapping balls
        let ball_coord_px = Vec2D::new(30., 30.);
        let ball_radius_px = WindowDefinition::len_to_px(main_content.new_ball.get_ball_size());
        let ball_radius = main_content.new_ball.get_ball_size();
        main_content
            .add_candidate_px(ball_coord_px.x, ball_coord_px.y)
            .unwrap();

        let ball_coord = main_content
            .win_definition
            .px_to_coord(ball_coord_px.x, ball_coord_px.y);
        let new_ball_coord = Vec2D::new(
            ball_coord.x + 0.5 * ball_radius,
            ball_coord.y + 0.5 * ball_radius,
        );
        let colliding_ball = main_content
            .get_game_logic_data()
            .get_ball_factory()
            .create_basic(new_ball_coord.clone(), ball_radius);
        // main_content. coord_to_px(&new_ball_coord),

        println!(
            "existing ball {:?} and new ball {:?} both with radius {}",
            ball_coord_px,
            main_content.win_definition.coord_to_px(&new_ball_coord),
            ball_radius_px
        );

        assert!(
            main_content.is_ball_colliding(&colliding_ball),
            "Collision not detected"
        );
    }

    #[test]
    fn test_coord_trafo_idempotent() {
        let main_content =
            MainContent::new(None, 500., 500., None).expect("could not create content");
        let test_coord = Vec2D::new(18., 234.);
        let vec = main_content
            .get_window_definition()
            .coord_to_px(&test_coord);
        let res_coord = main_content.win_definition.px_to_coord(vec.x, vec.y);

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
        let mut main_content =
            MainContent::new(None, 500., 500., None).expect("could not create content");

        // Add balls to the system
        let x1 = 80.0;
        let y1 = 80.0;
        let x2 = 105.0;
        let y2 = 105.0;
        let x3 = 200.0;
        let y3 = 200.0;

        let ball1_coord = main_content.win_definition.px_to_coord(x1, y1);
        let ball2_coord = main_content.win_definition.px_to_coord(x2, y2);
        let ball3_coord = main_content.win_definition.px_to_coord(x3, y3);

        println!(
            "b1 {:?} b2 {:?} b3 {:?}",
            ball1_coord, ball2_coord, ball3_coord
        );
        main_content.add_candidate_px(x1, y1).unwrap();
        main_content.add_candidate_px(x2, y2).unwrap();
        main_content.add_candidate_px(x3, y3).unwrap();

        // Initial state check
        {
            let ball1: &Ball2D = &*main_content.v_points[0].borrow();
            let ball2: &Ball2D = &*main_content.v_points[1].borrow();
            let ball3: &Ball2D = &*main_content.v_points[2].borrow();

            assert!(
                ball1.get_neighbour_names().is_empty(),
                "Ball 1 should have no neighbours initially"
            );
            assert!(
                ball2.get_neighbour_names().is_empty(),
                "Ball 2 should have no neighbours initially"
            );
            assert!(
                ball3.get_neighbour_names().is_empty(),
                "Ball 3 should have no neighbours initially"
            );
        }

        // Call collect_neighbours with the appropriate timestep (let's assume step_count is divisible by 10)
        {
            main_content.step_count = 10;
            println!("--------------------------------");
            main_content.collect_neighbours(DT_GOAL);
            println!("--------------------------------");
            main_content.collect_neighbours(DT_GOAL);

            // After collecting neighbors, Ball1 and Ball2 should be neighbors
            let ball1: &Ball2D = &*main_content.v_points[0].borrow();
            let ball2: &Ball2D = &*main_content.v_points[1].borrow();
            let ball3: &Ball2D = &*main_content.v_points[2].borrow();

            println!("ball1.neigh: {:?}", ball1.get_neighbour_names());
            println!("ball2.neigh: {:?}", ball2.get_neighbour_names());
            println!("ball3.neigh: {:?}", ball3.get_neighbour_names());

            assert_eq!(
                ball1.get_neighbour_names().len(),
                1,
                "Ball 1 should have 1 neighbour"
            );
            assert_eq!(
                ball2.get_neighbour_names().len(),
                1,
                "Ball 2 should have 1 neighbour"
            );
            assert!(
                ball1.get_neighbour_names().contains(&ball2.get_name()),
                "Ball 1 should be neighbour of Ball 2"
            );
            assert!(
                ball2.get_neighbour_names().contains(&ball1.get_name()),
                "Ball 2 should be neighbour of Ball 1"
            );

            assert!(
                ball3.get_neighbour_names().is_empty(),
                "Ball 3 should have no neighbours because it is far from others"
            );
        }
    }
}
