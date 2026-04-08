use crate::application::MainContent;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::{Rect, Vec2D};

fn make_content() -> MainContent {
    MainContent::new(Rect::new(-20.0, 20.0, 40.0, -40.0)).expect("create MainContent")
}

#[test]
fn add_and_toggle_valve_segment() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));

    mc.add_valve_segment(0, 0.8, 0.2, false);
    assert_eq!(mc.valve_segments.len(), 1);
    let v = mc.valve_segments[0];
    assert_eq!(v.line_id, 0);
    assert!((v.t_start - 0.2).abs() < 1e-6);
    assert!((v.t_end - 0.8).abs() < 1e-6);
    assert!(!v.is_closed);

    mc.set_valve_closed(v.id, true);
    assert!(mc.valve_segments[0].is_closed);
}

#[test]
fn open_valve_allows_pass_through() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    // Full-line valve section, open.
    mc.add_valve_segment(0, 0.0, 1.0, false);

    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.5), (0, 1.0));
    ball.v = Vec2D::new(0.0, -5.0);
    mc.balls.push(ball);

    let _ = mc.resolve_line_segment_collisions();

    // No reflection because open valve creates pass-through section.
    assert!(mc.balls[0].v.y < 0.0);
}

#[test]
fn closed_valve_reflects_like_wall() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    // Full-line valve section, closed.
    mc.add_valve_segment(0, 0.0, 1.0, true);

    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.5), (0, 1.0));
    ball.v = Vec2D::new(0.0, -5.0);
    mc.balls.push(ball);

    let _ = mc.resolve_line_segment_collisions();

    assert!(mc.balls[0].v.y > 0.0);
}

#[test]
fn closing_valve_pushes_overlapping_ball_out() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    mc.add_valve_segment(0, 0.0, 1.0, false);

    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.1), (0, 1.0));
    ball.v = Vec2D::new(0.0, 0.0);
    mc.balls.push(ball);

    let id = mc.valve_segments[0].id;
    mc.set_valve_closed(id, true);

    assert!(mc.balls[0].get_position().y.abs() >= 1.0);
}

#[test]
fn clear_points_clears_valves() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    mc.add_valve_segment(0, 0.25, 0.75, true);
    assert_eq!(mc.valve_segments.len(), 1);

    mc.clear_points();
    assert!(mc.valve_segments.is_empty());
}

#[test]
fn toggle_valve_flips_closed_state() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    mc.add_valve_segment(0, 0.2, 0.8, true);

    let id = mc.valve_segments[0].id;
    assert!(mc.valve_segments[0].is_closed);

    mc.toggle_valve(id);
    assert!(!mc.valve_segments[0].is_closed);

    mc.toggle_valve(id);
    assert!(mc.valve_segments[0].is_closed);
}

#[test]
fn move_valve_segment_updates_positions() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    mc.add_valve_segment(0, 0.2, 0.5, true);

    let id = mc.valve_segments[0].id;
    mc.move_valve_segment(id, 0.4, 0.9);

    assert!((mc.valve_segments[0].t_start - 0.4).abs() < 1e-6);
    assert!((mc.valve_segments[0].t_end - 0.9).abs() < 1e-6);
}

#[test]
fn snapshot_includes_valve_segments() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    mc.add_valve_segment(0, 0.1, 0.6, true);
    mc.add_valve_segment(0, 0.7, 0.9, false);

    let snap = mc.create_snapshot();
    assert_eq!(snap.valve_segments.len(), 2);
    assert_eq!(snap.valve_segments[0].line_id, 0);
    assert!(snap.valve_segments[0].is_closed);
    assert!(!snap.valve_segments[1].is_closed);
}
