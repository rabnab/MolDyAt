use super::{DT_GOAL, create_main_content, create_main_content_for_region};
use crate::application::GameLogic;
use crate::objects2d::ball_impl::{Ball2D, BallFactory, Named};
use crate::objects2d::{Vec2D, VerboseInfoMode};

#[test]
fn test_global_update() {
    let (mut main_content, _win_def) = create_main_content();

    // Add some balls

    let ball_radius = BallFactory::new().get_new_ball_radius();

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
        main_content.world().sim.get_kinetic_energy() > 0.0,
        "Kinetic energy should be positive but was {}",
        main_content.world().sim.get_kinetic_energy()
    );
    //assert!(main_content.sim.energy_pot >= 0.0, "Potential energy should be non-negative");
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
        let balls = &main_content.world().balls;
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
        main_content.world_mut().step_count = 10;
        println!("--------------------------------");
        main_content.collect_neighbours_by_cell(DT_GOAL);
        println!("--------------------------------");
        main_content.collect_neighbours_by_cell(DT_GOAL);

        // After collecting neighbors, Ball1 and Ball2 should be neighbors
        let balls = &main_content.world().balls;
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
    main_content.world_mut().step_count = 10;
    main_content.collect_neighbours_by_cell(DT_GOAL);
    main_content.collect_neighbours_by_cell(DT_GOAL);

    // Verify initial neighbor relationships exist
    {
        let balls = &main_content.world().balls;
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
    main_content.world_mut().balls[1].set_position_and_update_tail(Vec2D::new(f32::NAN, f32::NAN));

    // Remove invalid balls - this should:
    // 1. Remove ball at index 1
    // 2. Ball at index 2 moves to index 1
    // 3. Clear all neighbor lists to prevent stale indices
    main_content.remove_invalid_balls();

    // Verify the invalid ball was removed
    assert_eq!(
        main_content.world().balls.len(),
        2,
        "Should have 2 balls after removing 1 invalid ball"
    );

    // Verify all neighbor lists are cleared
    {
        let balls = &main_content.world().balls;
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
        let balls = &main_content.world().balls;
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
        let balls = &main_content.world().balls;
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
