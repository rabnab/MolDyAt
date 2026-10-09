// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use rust_thermo_core::application::renderer_snapshot::{OperatorSnapshot, ValveSpan, WallSnapshot};
use rust_thermo_core::objects2d::line_segment::{THICKNESS_TO_WORLD, valve_half_width};
use rust_thermo_core::objects2d::wall_chain::ObjectRef;
use rust_thermo_core::objects2d::{SOURCE_SPHERE_RADIUS, SimLineSegment, SimSourceSphere, Vec2D};

use crate::wgpu::pipelines::highlight_pipeline::highlight_capsule;

const EXTENT: f32 = 40.0;

fn wall_with_valve() -> OperatorSnapshot {
    let mut thick = SimLineSegment::new(1, Vec2D::new(2.0, 0.0), Vec2D::new(10.0, 0.0));
    thick.thickness = 6.0;
    OperatorSnapshot {
        line_segments: vec![
            SimLineSegment::new(0, Vec2D::new(-10.0, 0.0), Vec2D::new(-2.0, 0.0)),
            thick,
        ],
        walls: vec![WallSnapshot {
            chain_id: 4,
            start: Vec2D::new(-10.0, 0.0),
            end: Vec2D::new(10.0, 0.0),
            piece_ids: vec![0, 1],
            valve_ids: vec![3],
        }],
        valve_spans: vec![ValveSpan {
            valve_id: 3,
            p0: Vec2D::new(-2.0, 0.0),
            p1: Vec2D::new(2.0, 0.0),
            thickness: 6.0,
            is_closed: true,
        }],
        source_spheres: vec![SimSourceSphere::new(6, Vec2D::new(8.0, 8.0))],
        ..Default::default()
    }
}

#[test]
fn wall_highlight_spans_the_whole_wall_and_encloses_its_valves() {
    let ops = wall_with_valve();
    let (p0, p1, radius) =
        highlight_capsule(&ops, ObjectRef::Wall(4), EXTENT).expect("wall is present");

    assert_eq!((p0, p1), (Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0)));
    let valve_half = valve_half_width(6.0, EXTENT);
    assert!(valve_half > 0.5 * 6.0 * THICKNESS_TO_WORLD);
    assert!((radius - valve_half).abs() < 1e-6);
}

#[test]
fn valve_and_sphere_highlights_follow_their_shapes() {
    let ops = wall_with_valve();
    let (p0, p1, _) = highlight_capsule(&ops, ObjectRef::Valve(3), EXTENT).expect("valve");
    assert_eq!((p0, p1), (Vec2D::new(-2.0, 0.0), Vec2D::new(2.0, 0.0)));

    let (c0, c1, r) = highlight_capsule(&ops, ObjectRef::SourceSphere(6), EXTENT).expect("sphere");
    assert_eq!((c0, c1), (Vec2D::new(8.0, 8.0), Vec2D::new(8.0, 8.0)));
    assert!((r - SOURCE_SPHERE_RADIUS).abs() < 1e-6);
}

#[test]
fn deleted_objects_have_no_highlight() {
    let ops = wall_with_valve();
    assert!(highlight_capsule(&ops, ObjectRef::Wall(9), EXTENT).is_none());
    assert!(highlight_capsule(&ops, ObjectRef::Thermostat(0), EXTENT).is_none());
}
