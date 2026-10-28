// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Geometry of the flow arrows (#342), without a device.

use crate::wgpu::render::pipelines::flow_arrow_pipeline::flow_arrow_vertices;
use thermo_physics::application::thermo_field::CellFields;
use thermo_runtime::application::renderer_snapshot::ThermoGridSpec;

/// A 1 x 2 grid of 10 x 10 cells from the origin.
fn spec() -> ThermoGridSpec {
    ThermoGridSpec {
        rows: 1,
        cols: 2,
        cell_width: 10.0,
        cell_height: 10.0,
        origin_x: 0.0,
        origin_y: 0.0,
    }
}

fn cells(vx: [f32; 2], vy: [f32; 2]) -> CellFields {
    let mut cells = CellFields::filled(1, 2, 0.0);
    for (c, (x, y)) in vx.into_iter().zip(vy).enumerate() {
        cells.velocity_x.set(0, c, x);
        cells.velocity_y.set(0, c, y);
    }
    cells.update_flow_speed();
    cells
}

#[test]
fn the_fastest_cell_gets_the_longest_arrow_along_its_flow() {
    let mut verts = Vec::new();
    flow_arrow_vertices(&cells([2.0, 0.0], [0.0, 1.0]), &spec(), &mut verts);
    // Shaft and two head strokes per cell, two vertices each.
    assert_eq!(verts.len(), 2 * 6);

    // Cell 0 flows +x at full length (0.8 of the cell), centred at (5, 5).
    let [tail, tip] = [verts[0].position, verts[1].position];
    assert!((tail[0] - 1.0).abs() < 1e-5 && (tip[0] - 9.0).abs() < 1e-5);
    assert_eq!((tail[1], tip[1]), (5.0, 5.0));
    // The head strokes start at the tip and point back.
    assert_eq!(verts[2].position, tip);
    assert!(verts[3].position[0] < tip[0]);

    // Cell 1 flows +y at half the speed: half the length, centred at (15, 5).
    let [tail, tip] = [verts[6].position, verts[7].position];
    assert!((tail[0] - 15.0).abs() < 1e-5 && (tip[0] - 15.0).abs() < 1e-5);
    assert!((tail[1] - 3.0).abs() < 1e-5 && (tip[1] - 7.0).abs() < 1e-5);
}

#[test]
fn cells_without_flow_get_no_arrow() {
    let mut verts = Vec::new();
    let mut fields = cells([1.0, 0.0], [0.0, 0.0]);
    fields.velocity_x.set(0, 1, f32::NAN);
    fields.update_flow_speed();
    flow_arrow_vertices(&fields, &spec(), &mut verts);
    assert_eq!(verts.len(), 6, "only the moving cell");

    flow_arrow_vertices(&CellFields::filled(1, 2, 0.0), &spec(), &mut verts);
    assert!(verts.is_empty(), "a resting gas shows no arrows");
}
