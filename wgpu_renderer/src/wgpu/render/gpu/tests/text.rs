// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::wgpu::render::gpu::text::{
    ASCII_CHARS, GLYPH_H, GLYPH_W, TextVertex, build_atlas_data, push_label_vertices,
    window_glyph_ndc_size,
};

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
    push_label_vertices(&mut verts, "1.2", 0.0, 0.0, 0.1, 0.2);
    assert_eq!(verts.len(), 3 * 6);
}

#[test]
fn unknown_glyphs_are_skipped_but_keep_their_space() {
    let mut verts: Vec<TextVertex> = Vec::new();
    push_label_vertices(&mut verts, "1Ü2", 0.0, 0.0, 0.1, 0.2);
    assert_eq!(verts.len(), 2 * 6);
    // '2' is the third character: its left edge sits two glyphs right of the start.
    let left_of_two = verts[6].position[0];
    assert!((left_of_two - (-0.15 + 0.2)).abs() < 1e-6);
}

#[test]
fn label_is_centred_on_the_anchor() {
    let mut verts: Vec<TextVertex> = Vec::new();
    push_label_vertices(&mut verts, "12", 0.5, -0.25, 0.1, 0.2);
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

/// The ASCII atlas is indexed by character code; the result matches a
/// search, and characters outside the atlas are skipped.
#[test]
fn ascii_atlas_lookup_matches_a_search() {
    let mut verts = Vec::new();
    push_label_vertices(&mut verts, "A~Ü ", 0.0, 0.0, 0.1, 0.1);
    // Six vertices per drawn glyph: 'A' and '~'; not 'Ü', and a space
    // draws nothing.
    assert_eq!(verts.len(), 12);
    let u0 = |glyph: usize| verts[glyph * 6].uv[0];
    let n = ASCII_CHARS.len() as f32;
    for (glyph, ch) in [(0, 'A'), (1, '~')] {
        let idx = ASCII_CHARS.iter().position(|&c| c == ch).unwrap();
        assert_eq!(u0(glyph), idx as f32 / n, "{ch}");
    }
}

/// The shared atlas (#303) has a glyph for every character the former
/// small atlases of the value and thermostat labels held, so those labels
/// look the same.
#[test]
fn the_ascii_atlas_holds_the_label_characters() {
    let atlas = build_atlas_data(&ASCII_CHARS);
    let width = ASCII_CHARS.len() * GLYPH_W as usize;
    assert_eq!(atlas.len(), width * GLYPH_H as usize);
    for ch in "0123456789.-:LTOF".chars() {
        let x0 = (ch as usize - ' ' as usize) * GLYPH_W as usize;
        let lit = (0..GLYPH_H as usize)
            .flat_map(|row| (0..GLYPH_W as usize).map(move |col| (row, col)))
            .filter(|&(row, col)| atlas[row * width + x0 + col] == 255)
            .count();
        assert!(lit > 0, "{ch} has no pixels");
    }
}
