// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The wall being drawn or dragged (ghost) collides like a real wall, and a
//! wall that appears or changes on top of balls lets them leave on their own
//! path instead of moving them.

use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;
use crate::application::operator_command::line::SetGhostWall;
use crate::objects2d::ball_impl::{Ball2D, SPECIES_M};
use crate::objects2d::line_segment::GhostWall;
use crate::objects2d::{Rect, Vec2D};

fn make_content() -> MainContent {
    crate::application::core::tests::new_main_content(Rect::new(-20.0, 20.0, 40.0, -40.0))
        .expect("create MainContent")
}

/// Horizontal ghost along y = 0 from x = -10 to 10, thickness 1 (half 0.5).
fn ghost() -> GhostWall {
    GhostWall {
        start: Vec2D::new(-10.0, 0.0),
        end: Vec2D::new(10.0, 0.0),
        thickness: 1.0,
    }
}

fn push_ball(mc: &mut MainContent, pos: Vec2D, v: Vec2D) {
    let mut ball = Ball2D::new(1, pos, SPECIES_M);
    ball.v = v;
    mc.world_mut().balls.push(ball);
}

/// Step positions by `v * dt` and resolve wall collisions, `steps` times.
fn move_and_collide(mc: &mut MainContent, steps: usize, dt: f32) {
    for _ in 0..steps {
        for ball in &mut mc.world_mut().balls {
            let next = ball.get_position().add_scaled(&ball.v, dt);
            ball.set_position_and_update_tail(next);
        }
        let _ = mc.resolve_line_segment_collisions();
    }
}

#[test]
fn a_ghost_landing_on_a_ball_flags_it_without_moving_it() {
    let mut mc = make_content();
    push_ball(&mut mc, Vec2D::new(0.0, 0.2), Vec2D::zero());
    push_ball(&mut mc, Vec2D::new(0.0, 6.0), Vec2D::zero());

    mc.set_ghost_wall(Some(ghost()));

    let balls = &mc.world().balls;
    assert_eq!(*balls[0].get_position(), Vec2D::new(0.0, 0.2));
    assert!(balls[0].leaving_wall);
    assert!(!balls[1].leaving_wall);
}

#[test]
fn ghost_reflects_balls_like_a_wall_but_adds_no_wall_momentum() {
    let mut mc = make_content();
    mc.set_ghost_wall(Some(ghost()));
    push_ball(&mut mc, Vec2D::new(0.0, 1.0), Vec2D::new(0.0, -5.0));

    let (px, py) = mc.resolve_line_segment_collisions();

    assert!(mc.world().balls[0].v.y > 0.0);
    assert_eq!((px, py), (0.0, 0.0));
}

#[test]
fn balls_are_not_spawned_inside_a_ghost() {
    let mut mc = make_content();
    mc.set_ghost_wall(Some(ghost()));

    assert!(mc.add_candidate_at_pos(Vec2D::new(3.0, 0.0)).is_err());
    assert!(mc.add_candidate_at_pos(Vec2D::new(3.0, 8.0)).is_ok());
}

#[test]
fn clearing_the_ghost_stops_collisions() {
    let mut mc = make_content();
    mc.set_ghost_wall(Some(ghost()));
    Box::new(SetGhostWall { ghost: None }).execute(&mut mc);
    push_ball(&mut mc, Vec2D::new(0.0, 1.0), Vec2D::new(0.0, -5.0));

    let _ = mc.resolve_line_segment_collisions();

    assert!(mc.world().balls[0].v.y < 0.0);
}

#[test]
fn a_ball_under_a_new_wall_leaves_with_unchanged_velocity() {
    let mut mc = make_content();
    mc.set_ghost_wall(Some(ghost()));
    push_ball(&mut mc, Vec2D::new(-4.0, -0.1), Vec2D::new(0.5, -4.0));

    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    assert!(mc.world().ghost_wall.is_none());
    assert!(mc.world().balls[0].leaving_wall);

    move_and_collide(&mut mc, 30, 0.02);

    let ball = &mc.world().balls[0];
    assert!(
        ball.get_position().y < -(ball.contact_radius + 0.5),
        "{ball:?}"
    );
    assert_eq!(ball.v, Vec2D::new(0.5, -4.0));
    assert!(!ball.leaving_wall, "flag clears once the ball is free");
}

#[test]
fn moving_a_wall_endpoint_onto_a_ball_flags_it() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 5.0), Vec2D::new(10.0, 5.0));
    push_ball(&mut mc, Vec2D::new(0.0, 0.0), Vec2D::zero());

    // Drag the end from (10, 5) down to (10, -5): the wall now crosses (0, 0).
    mc.move_line_segment_endpoint(0, Vec2D::new(10.0, -5.0), false);

    assert!(mc.world().ghost_wall.is_none());
    assert!(mc.world().balls[0].leaving_wall);
    assert_eq!(*mc.world().balls[0].get_position(), Vec2D::zero());
}

#[test]
fn thickening_a_wall_onto_a_ball_flags_it() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    push_ball(&mut mc, Vec2D::new(0.0, 2.5), Vec2D::zero());
    assert!(!mc.world().balls[0].leaving_wall);

    mc.change_thickness(0, 3); // half-thickness 0.5 -> 2.0

    assert!(mc.world().balls[0].leaving_wall);
}

#[test]
fn a_fast_ball_whose_centre_tunnelled_into_a_wall_is_pushed_back() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    // Not flagged: it came from above in one big step.
    push_ball(&mut mc, Vec2D::new(0.0, 0.2), Vec2D::new(0.0, -50.0));

    let _ = mc.resolve_line_segment_collisions();

    let ball = &mc.world().balls[0];
    assert!(ball.v.y > 0.0, "reflected");
    assert!(
        ball.get_position().y >= ball.contact_radius + 0.5,
        "pushed back out"
    );
}

#[test]
fn resizing_or_clearing_drops_the_ghost() {
    let mut mc = make_content();
    mc.set_ghost_wall(Some(ghost()));
    let old = Rect::new(-20.0, 20.0, 40.0, -40.0);
    let new = Rect::new(-40.0, 40.0, 80.0, -80.0);
    mc.rescale_all_operators(&old, &new);
    assert!(mc.world().ghost_wall.is_none());

    mc.set_ghost_wall(Some(ghost()));
    mc.clear_all_operators();
    assert!(mc.world().ghost_wall.is_none());
}
