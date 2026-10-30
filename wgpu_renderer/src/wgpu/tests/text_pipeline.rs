// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::wgpu::MARGINS;
use crate::wgpu::render::pipelines::text_pipeline::cursor_text_vertices;

#[test]
fn cursor_text_emits_one_quad_per_character() {
    let verts = cursor_text_vertices("(1.00, 2.00)  1g", (1280, 720), MARGINS);
    assert_eq!(verts.len(), 16 * 6);
}

#[test]
fn cursor_text_sits_in_the_bottom_margin_right_of_the_pane() {
    let (w, h) = (1280.0_f32, 720.0_f32);
    let verts = cursor_text_vertices("(1.00, 2.00)", (w as u32, h as u32), MARGINS);

    let pane_right_ndc = ((w - MARGINS.right) / w) * 2.0 - 1.0;
    let pane_bottom_ndc = 1.0 - ((h - MARGINS.bottom) / h) * 2.0;
    for v in &verts {
        assert!(
            v.position[0] <= pane_right_ndc + 1e-5,
            "past the pane's right edge"
        );
        assert!(
            v.position[1] <= pane_bottom_ndc + 1e-5,
            "above the pane's bottom edge"
        );
        assert!(v.position[1] >= -1.0, "below the window");
    }
}

/// The cursor readout names the species of new balls (#238).
#[test]
fn readout_names_the_species_of_new_balls() {
    use crate::wgpu::render::pipelines::text_pipeline::cursor_readout;
    use thermo_runtime::application::renderer_snapshot::RendererSnapshot;
    let mut snapshot = RendererSnapshot::new();
    assert_eq!(
        cursor_readout((1.0, 2.0), &snapshot.frame, &snapshot.balls),
        "(1.00, 2.00)  0g  N2"
    );
    snapshot.balls.cursor_species = snapshot.balls.species.by_name("Ne").unwrap();
    snapshot.frame.gravity_force_multiple = 1;
    assert_eq!(
        cursor_readout((1.0, 2.0), &snapshot.frame, &snapshot.balls),
        "(1.00, 2.00)  1g  Ne"
    );
}
