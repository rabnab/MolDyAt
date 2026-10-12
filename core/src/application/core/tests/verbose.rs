// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::create_main_content;
use crate::application::MainContent;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::BallFactory;

/// World with two balls; returns it with the ID of the first ball.
fn two_ball_content() -> (MainContent, usize) {
    let (mut main_content, _win_def) = create_main_content();
    let ball_radius = BallFactory::new().get_new_ball_radius();
    main_content.add_candidate_at_pos(Vec2D::zero()).unwrap();
    main_content
        .add_candidate_at_pos(Vec2D::new(1., 1.) * ball_radius * 2.1)
        .unwrap();
    let first_id = main_content.world().balls[0].get_id();
    (main_content, first_id)
}

fn console_ball_count(main_content: &MainContent) -> usize {
    let balls = &main_content.world().balls;
    balls.iter().filter(|b| b.verbose.console).count()
}

#[test]
fn test_verbosity_input_reports_console_mode_only_when_switched_on() {
    let (mut main_content, first_id) = two_ball_content();

    assert!(main_content.apply_verbosity_input(&format!("{first_id}c\n")));
    assert_eq!(console_ball_count(&main_content), 1);

    // Switching the same ball off again enables nothing.
    assert!(!main_content.apply_verbosity_input(&format!("{first_id}c\n")));
    assert_eq!(console_ball_count(&main_content), 0);

    assert!(main_content.apply_verbosity_input("ac\n"));
    assert_eq!(console_ball_count(&main_content), 2);
}

#[test]
fn test_verbosity_input_without_console_ball_reports_nothing_switched_on() {
    let (mut main_content, first_id) = two_ball_content();
    main_content.apply_verbosity_input("ac\n");

    // Clear all, even though the input ends in the console mode letter.
    assert!(!main_content.apply_verbosity_input("0c\n"));
    assert_eq!(console_ball_count(&main_content), 0);

    // Unknown ball ID, invalid input and a non-console mode.
    assert!(!main_content.apply_verbosity_input("999999c\n"));
    assert!(!main_content.apply_verbosity_input("xc\n"));
    assert!(!main_content.apply_verbosity_input("c\n"));
    assert!(!main_content.apply_verbosity_input(&format!("{first_id}s\n")));
    assert_eq!(console_ball_count(&main_content), 0);
}
