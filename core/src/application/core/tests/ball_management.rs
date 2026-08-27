use super::{create_main_content, create_main_content_for_region};
use crate::application::GameLogic;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{Ball2D, BallFactory};
use float_cmp::approx_eq;

#[test]
fn test_add_candidate() {
    let (mut main_content, _) = create_main_content();

    let new_ball_size = BallFactory::new().get_new_ball_radius();
    // Add a valid candidate
    let coord = main_content.world().world_region_inset(new_ball_size * 1.1);
    let (x, y) = coord.get_tl();

    println!(
        "test adding ball at world ({},{}) with radius {new_ball_size} in inset {:?}",
        x,
        y,
        main_content.world().world_region_inset(0.)
    );

    main_content
        .add_candidate_at_pos((x, y).into())
        .expect("Could not add candidate");

    assert_eq!(main_content.world().balls.len(), 1);

    // Ensure the added ball is in the correct position
    {
        let ball = &main_content.world().balls[0];
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
        let coord = main_content.world().world_region();
        main_content
            .add_candidate_at_pos((coord.get_tl().0 - 0.1, coord.get_tl().1 - 0.1).into())
            .expect_err("Could not add candidate");
        assert_eq!(
            main_content.world().balls.len(),
            1,
            "Ball added out of valid region"
        );
    }
}

#[test]
fn test_is_ball_in_region() {
    let (main_content, _) = create_main_content();

    let tl = main_content.world().world_region().get_tl();
    let ball_coord = (0., 0.);
    println!("ball coordinates are {:?}", ball_coord,);
    let ball_radius = 5.;
    let ball = Ball2D::new(0, ball_coord.into(), (0, ball_radius));
    assert!(main_content.is_ball_in_region(&ball));

    let invalid_ball = Ball2D::new(1, (tl.0 - 0.1, tl.1 - 0.1).into(), (0, ball_radius));
    // Move the ball outside the region
    assert!(!main_content.is_ball_in_region(&invalid_ball));
}

#[test]
fn test_ball_collision() {
    let (mut main_content, _win_def) = create_main_content();

    // Add two overlapping balls

    let factory = BallFactory::new();
    let ball_radius = factory.get_new_ball_radius();

    let ball_coord = Vec2D::zero();
    main_content
        .add_candidate_at_pos(ball_coord)
        .unwrap_or_else(|_| panic!("could not add ball at {:?}", ball_coord));

    let new_ball_coord = Vec2D::new(
        ball_coord.x + 0.5 * ball_radius,
        ball_coord.y + 0.5 * ball_radius,
    );
    let colliding_ball = factory.create_massive_with_velocity(new_ball_coord, Vec2D::zero());
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
