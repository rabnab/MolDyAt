// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Snapshot tests moved from `thermo_physics` `core/tests/valves.rs` (#308).

use crate::application::CreateSnapshot;
use crate::application::MainContent;
use crate::objects2d::{Rect, Vec2D};

fn make_content() -> MainContent {
    thermo_physics::test_support::new_main_content(Rect::new(-20.0, 20.0, 40.0, -40.0))
        .expect("create MainContent")
}

/// Wall from (-10, 0) to (10, 0): line id 0, chain id 0.
fn make_content_with_wall() -> MainContent {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    mc
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-4,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn snapshot_includes_valve_spans() {
    let mut mc = make_content_with_wall();
    // First valve: chain fractions 0.1..0.6. The remainder of the wall is
    // then piece 1, covering chain fractions 0.6..1.0.
    mc.add_valve_segment(0, 0.1, 0.6, true);
    // Second valve on piece 1: chain fractions 0.7..0.9 = local 0.25..0.75.
    mc.add_valve_segment(1, 0.25, 0.75, false);

    let snap = mc.create_snapshot();
    let spans = &snap.operators.valve_spans;
    assert_eq!(spans.len(), 2);
    assert!(spans[0].is_closed);
    assert!(!spans[1].is_closed);
    assert_close(spans[0].p0.x, -8.0);
    assert_close(spans[0].p1.x, 2.0);
    assert_close(spans[1].p0.x, 4.0);
    assert_close(spans[1].p1.x, 8.0);
}

#[test]
fn valve_thickness_is_thicker_flanking_piece() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.25, 0.75, true);
    let id = mc.world().valve_segments().valves()[0].id;
    assert_close(
        mc.world().valve_thickness(id).expect("valve thickness"),
        1.0,
    );

    // Only the trailing piece (line id 1) gets thicker.
    mc.change_thickness(1, 2);
    assert_close(
        mc.world().valve_thickness(id).expect("valve thickness"),
        3.0,
    );

    let snap = mc.create_snapshot();
    assert_close(snap.operators.valve_spans[0].thickness, 3.0);
}

#[test]
fn snapshot_wall_endpoints_skip_valve_boundaries() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.25, 0.75, true);

    let snap = mc.create_snapshot();
    assert_eq!(snap.operators.line_segments.len(), 2);
    assert_eq!(snap.operators.walls.len(), 1);
    let wall = &snap.operators.walls[0];
    assert_close(wall.start.x, -10.0);
    assert_close(wall.end.x, 10.0);
    assert_eq!(wall.piece_ids, vec![0, 1]);
    let valve_id = snap.operators.valve_spans[0].valve_id;
    assert_eq!(wall.valve_ids, vec![valve_id]);
}
