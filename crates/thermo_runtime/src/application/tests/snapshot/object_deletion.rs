// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Snapshot tests moved from `thermo_physics` `core/tests/object_deletion.rs` (#308).

use crate::application::CreateSnapshot;
use crate::application::MainContent;
use crate::objects2d::wall_chain::{Joint, ObjectRef};
use crate::objects2d::{Rect, Vec2D};

/// Wall 0: (-10, 0)-(10, 0), split by a closed valve at chain fractions
/// 0.25..0.75 into pieces 0 and 1. Wall 1: (-10, 5)-(10, 5), unsplit (piece 2).
fn make_two_walls_one_valve_for_deletion() -> (MainContent, usize) {
    let mut mc = make_deletion_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    mc.add_valve_segment(0, 0.25, 0.75, true);
    mc.add_line_segment(Vec2D::new(-10.0, 5.0), Vec2D::new(10.0, 5.0));
    let valve_id = mc.world().valve_segments.valves()[0].id;
    (mc, valve_id)
}

fn make_deletion_content() -> MainContent {
    thermo_physics::test_support::new_main_content(Rect::new(-20.0, 20.0, 40.0, -40.0))
        .expect("create MainContent")
}

#[test]
fn deleting_valve_leaves_a_gap_and_keeps_the_pieces() {
    let (mut mc, valve_id) = make_two_walls_one_valve_for_deletion();

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
