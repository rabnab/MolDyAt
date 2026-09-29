use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;
use crate::application::operator_command::delete::DeleteObject;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::wall_chain::{Joint, ObjectRef};
use crate::objects2d::{Rect, Vec2D};

fn make_content() -> MainContent {
    MainContent::new(Rect::new(-20.0, 20.0, 40.0, -40.0)).expect("create MainContent")
}

/// Wall 0: (-10, 0)-(10, 0), split by a closed valve at chain fractions
/// 0.25..0.75 into pieces 0 and 1. Wall 1: (-10, 5)-(10, 5), unsplit (piece 2).
fn make_two_walls_one_valve() -> (MainContent, usize) {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    mc.add_valve_segment(0, 0.25, 0.75, true);
    mc.add_line_segment(Vec2D::new(-10.0, 5.0), Vec2D::new(10.0, 5.0));
    let valve_id = mc.world().valve_segments.valves()[0].id;
    (mc, valve_id)
}

#[test]
fn deleting_wall_removes_its_pieces_and_valves_only() {
    let (mut mc, _) = make_two_walls_one_valve();

    mc.delete_object(ObjectRef::Wall(0));

    let world = mc.world();
    assert!(world.wall_chains.find(0).is_none());
    assert!(world.valve_segments.valves().is_empty());
    let remaining: Vec<usize> = world
        .line_segments
        .segments()
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(remaining, vec![2]);
    assert!(world.wall_chains.find(1).is_some());
}

#[test]
fn deleted_wall_no_longer_reflects_balls() {
    let (mut mc, _) = make_two_walls_one_valve();
    mc.delete_object(ObjectRef::Wall(0));

    let mut ball = Ball2D::new(1, Vec2D::new(-8.0, 0.5), (0, 1.0));
    ball.v = Vec2D::new(0.0, -5.0);
    mc.world_mut().balls.push(ball);
    let _ = mc.resolve_line_segment_collisions();

    assert!(mc.world().balls[0].v.y < 0.0);
}

#[test]
fn deleting_valve_leaves_a_gap_and_keeps_the_pieces() {
    let (mut mc, valve_id) = make_two_walls_one_valve();

    mc.delete_object(ObjectRef::Valve(valve_id));

    let world = mc.world();
    assert!(world.valve_segments.find(valve_id).is_none());
    let chain = world.wall_chains.find(0).expect("wall stays");
    assert_eq!(chain.pieces.len(), 2);
    assert!(matches!(
        chain.joints[0],
        Joint::Gap { t_start, t_end } if (t_start - 0.25).abs() < 1e-6 && (t_end - 0.75).abs() < 1e-6
    ));
    assert!(mc.create_snapshot().operators.valve_spans.is_empty());
}

#[test]
fn deleting_the_same_valve_twice_keeps_the_gap() {
    let (mut mc, valve_id) = make_two_walls_one_valve();
    mc.delete_object(ObjectRef::Valve(valve_id));
    mc.delete_object(ObjectRef::Valve(valve_id));

    let chain = mc.world().wall_chains.find(0).expect("wall stays");
    assert_eq!(chain.pieces.len(), 2);
    assert_eq!(chain.joints.len(), 1);
    assert!(matches!(chain.joints[0], Joint::Gap { .. }));
    assert_eq!(mc.world().line_segments.segments().len(), 3);
}

#[test]
fn gap_left_by_deleted_closed_valve_lets_balls_through() {
    let (mut mc, valve_id) = make_two_walls_one_valve();
    mc.delete_object(ObjectRef::Valve(valve_id));

    // Above the former valve (x = -5..5), heading down.
    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.5), (0, 1.0));
    ball.v = Vec2D::new(0.0, -5.0);
    mc.world_mut().balls.push(ball);
    let _ = mc.resolve_line_segment_collisions();

    assert!(mc.world().balls[0].v.y < 0.0);
}

#[test]
fn deleting_source_sphere_and_thermostat() {
    let mut mc = make_content();
    mc.add_source_sphere(Vec2D::new(1.0, 1.0));
    mc.add_source_sphere(Vec2D::new(5.0, 5.0));
    mc.add_thermostat(Vec2D::new(-3.0, -3.0));

    mc.delete_object(ObjectRef::SourceSphere(0));
    mc.delete_object(ObjectRef::Thermostat(0));

    let world = mc.world();
    let spheres: Vec<usize> = world
        .source_spheres
        .spheres()
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(spheres, vec![1]);
    assert!(world.thermostats.thermostats().is_empty());
}

#[test]
fn deleting_a_stale_reference_changes_nothing() {
    let (mut mc, valve_id) = make_two_walls_one_valve();
    mc.delete_object(ObjectRef::Wall(0));

    mc.delete_object(ObjectRef::Wall(0));
    mc.delete_object(ObjectRef::Valve(valve_id));
    mc.delete_object(ObjectRef::SourceSphere(7));
    mc.delete_object(ObjectRef::Thermostat(7));

    assert_eq!(mc.world().line_segments.segments().len(), 1);
    assert!(mc.world().wall_chains.find(1).is_some());
}

#[test]
fn delete_object_command_deletes_its_target() {
    let (mut mc, _) = make_two_walls_one_valve();

    Box::new(DeleteObject {
        target: ObjectRef::Wall(1),
    })
    .execute(&mut mc);

    assert!(mc.world().wall_chains.find(1).is_none());
    assert_eq!(mc.world().line_segments.segments().len(), 2);
}
