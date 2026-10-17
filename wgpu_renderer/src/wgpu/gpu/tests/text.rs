// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::wgpu::gpu::text::{TextVertex, push_label_vertices, window_glyph_ndc_size};

const CHARS: [char; 3] = ['1', '2', '.'];

#[test]
fn window_glyph_size_scales_the_8px_glyph_to_ndc() {
    // 8 px glyph at scale 2 = 16 px; NDC spans 2 over the window size.
    let (w, h) = window_glyph_ndc_size((800.0, 400.0), 2.0);
    assert!((w - 0.04).abs() < 1e-6, "{w}");
    assert!((h - 0.08).abs() < 1e-6, "{h}");
}

#[test]
fn each_known_glyph_emits_one_quad() {
    let mut verts: Vec<TextVertex> = Vec::new();
    push_label_vertices(&mut verts, &CHARS, "1.2", 0.0, 0.0, 0.1, 0.2);
    assert_eq!(verts.len(), 3 * 6);
}

#[test]
fn unknown_glyphs_are_skipped_but_keep_their_space() {
    let mut verts: Vec<TextVertex> = Vec::new();
    push_label_vertices(&mut verts, &CHARS, "1x2", 0.0, 0.0, 0.1, 0.2);
    assert_eq!(verts.len(), 2 * 6);
    // '2' is the third character: its left edge sits two glyphs right of the start.
    let left_of_two = verts[6].position[0];
    assert!((left_of_two - (-0.15 + 0.2)).abs() < 1e-6);
}

#[test]
fn label_is_centred_on_the_anchor() {
    let mut verts: Vec<TextVertex> = Vec::new();
    push_label_vertices(&mut verts, &CHARS, "12", 0.5, -0.25, 0.1, 0.2);
    let min_x = verts
        .iter()
        .map(|v| v.position[0])
        .fold(f32::INFINITY, f32::min);
    let max_x = verts
        .iter()
        .map(|v| v.position[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = verts
        .iter()
        .map(|v| v.position[1])
        .fold(f32::INFINITY, f32::min);
    let max_y = verts
        .iter()
        .map(|v| v.position[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(((min_x + max_x) * 0.5 - 0.5).abs() < 1e-6);
    assert!(((min_y + max_y) * 0.5 + 0.25).abs() < 1e-6);
}
