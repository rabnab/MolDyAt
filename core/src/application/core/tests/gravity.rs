// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::create_main_content;
use crate::application::sim_stats::{HIGH_GRAVITY, NORMAL_GRAVITY};
use crate::objects2d::Vec2D;

#[test]
fn test_toggle_gravity_cycles_off_normal_high_off() {
    let (mut main_content, _) = create_main_content();

    assert_eq!(main_content.world().physics.constants.gravity, 0.0);

    main_content.toggle_gravity();
    assert_eq!(
        main_content.world().physics.constants.gravity,
        NORMAL_GRAVITY
    );

    main_content.toggle_gravity();
    assert_eq!(main_content.world().physics.constants.gravity, HIGH_GRAVITY);

    main_content.toggle_gravity();
    assert_eq!(main_content.world().physics.constants.gravity, 0.0);
}

#[test]
fn test_toggle_gravity_treats_argon_unit_gravity_of_old_saves_as_normal() {
    let (mut main_content, _) = create_main_content();
    // 1 g as saves from before #236 store it (argon units).
    main_content.world_mut().physics.constants.gravity = 1.3157e-13;

    main_content.toggle_gravity();
    assert_eq!(main_content.world().physics.constants.gravity, HIGH_GRAVITY);

    main_content.toggle_gravity();
    assert_eq!(main_content.world().physics.constants.gravity, 0.0);
}

#[test]
fn test_snapshot_reports_gravity_force_multiple() {
    let (mut main_content, _) = create_main_content();

    assert_eq!(main_content.create_snapshot().gravity_force_multiple, 0);

    main_content.toggle_gravity();
    assert_eq!(main_content.create_snapshot().gravity_force_multiple, 1);

    main_content.toggle_gravity();
    assert_eq!(main_content.create_snapshot().gravity_force_multiple, 5);
}

#[test]
fn test_gravity_moves_stationary_ball_on_first_step_after_toggle() {
    let (mut main_content, _) = create_main_content();

    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("failed to add ball");
    main_content.toggle_gravity();

    let y_before = main_content.world().balls[0].get_position().y;

    main_content
        .simulation_step()
        .expect("simulation step should succeed");

    let ball = &main_content.world().balls[0];
    assert!(
        ball.get_position().y < y_before,
        "gravity should move the ball downward on the first step"
    );
    assert!(
        ball.v.y < 0.0,
        "gravity should accelerate the ball downward"
    );
}

#[test]
fn test_new_ball_added_under_gravity_starts_falling_immediately() {
    let (mut main_content, _) = create_main_content();

    main_content.toggle_gravity();
    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("failed to add ball");

    let y_before = main_content.world().balls[0].get_position().y;

    main_content
        .simulation_step()
        .expect("simulation step should succeed");

    let ball = &main_content.world().balls[0];
    assert!(
        ball.get_position().y < y_before,
        "new balls should include gravity in their first position update"
    );
    assert!(ball.v.y < 0.0, "new balls should gain downward velocity");
}
