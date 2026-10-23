// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Operator commands applied to a world: the commands are thin wrappers, so
//! these only check that each reaches its `MainContent` method.

use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;
use crate::application::operator_command::delete::DeleteObject;
use crate::application::operator_command::line::SetGhostWall;
use crate::objects2d::ball_impl::{Ball2D, SPECIES_M};
use crate::objects2d::line_segment::GhostWall;
use crate::objects2d::wall_chain::ObjectRef;
use crate::objects2d::{Rect, Vec2D};

// ── Ghost wall (from thermo_physics core/tests/ghost_wall.rs) ──

fn make_content() -> MainContent {
    thermo_physics::test_support::new_main_content(Rect::new(-20.0, 20.0, 40.0, -40.0))
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

#[test]
fn clearing_the_ghost_stops_collisions() {
    let mut mc = make_content();
    mc.set_ghost_wall(Some(ghost()));
    Box::new(SetGhostWall { ghost: None }).execute(&mut mc);
    push_ball(&mut mc, Vec2D::new(0.0, 1.0), Vec2D::new(0.0, -5.0));

    let _ = mc.resolve_line_segment_collisions();

    assert!(mc.world().balls[0].v.y < 0.0);
}

// ── Deletion (from thermo_physics core/tests/object_deletion.rs) ──

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
fn delete_object_command_deletes_its_target() {
    let (mut mc, _) = make_two_walls_one_valve();

    Box::new(DeleteObject {
        target: ObjectRef::Wall(1),
    })
    .execute(&mut mc);

    assert!(mc.world().wall_chains.find(1).is_none());
    assert_eq!(mc.world().line_segments.segments().len(), 2);
}
