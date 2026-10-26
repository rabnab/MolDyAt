// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! CPU-side tests of the energy plot pipeline: segment instances, glyph
//! vertices and the surface format choice (no GPU needed).

use thermo_ui::graphics::energy_plot::plot_layout::{BACKGROUND, GLYPH_CELL};
use thermo_ui::graphics::energy_plot::{PlotLabel, PlotScene, PlotSegment};

use crate::wgpu::gpu::text::ASCII_CHARS;
use crate::wgpu::pipelines::plot_pipeline::{push_label, segment_instances};
use crate::wgpu::plot_surface::plot_surface_format;

fn scene(segments: Vec<PlotSegment>, labels: Vec<PlotLabel>) -> PlotScene {
    PlotScene {
        size: (400, 300),
        background: BACKGROUND,
        plot_area: None,
        segments,
        labels,
    }
}

#[test]
fn segment_instances_carry_points_half_width_and_colour() {
    let color = [0.1, 0.2, 0.3, 1.0];
    let scene = scene(
        vec![PlotSegment {
            from: [1.0, 2.0],
            to: [30.0, 40.0],
            width: 3.0,
            color,
        }],
        Vec::new(),
    );
    let mut out = Vec::new();
    segment_instances(&scene, &mut out);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].p0, [1.0, 2.0]);
    assert_eq!(out[0].p1, [30.0, 40.0]);
    assert_eq!(out[0].half_width, 1.5);
    assert_eq!(out[0].color, color);
}

#[test]
fn label_gives_six_vertices_per_glyph_in_window_pixels() {
    let label = PlotLabel {
        text: "Ab".to_string(),
        origin: [10.0, 20.0],
        scale: 2.0,
        color: [1.0, 0.0, 0.0, 1.0],
    };
    let mut out = Vec::new();
    push_label(&label, &mut out);

    assert_eq!(out.len(), 12);
    let glyph = GLYPH_CELL * 2.0;
    let xs: Vec<f32> = out.iter().map(|v| v.position[0]).collect();
    let ys: Vec<f32> = out.iter().map(|v| v.position[1]).collect();
    assert_eq!(xs.iter().copied().fold(f32::MAX, f32::min), 10.0);
    assert_eq!(
        xs.iter().copied().fold(f32::MIN, f32::max),
        10.0 + 2.0 * glyph
    );
    assert_eq!(ys.iter().copied().fold(f32::MAX, f32::min), 20.0);
    assert_eq!(ys.iter().copied().fold(f32::MIN, f32::max), 20.0 + glyph);
    assert!(out.iter().all(|v| v.color == label.color));

    // 'A' is atlas cell 33 of the printable-ASCII atlas.
    let u0 = ('A' as usize - ' ' as usize) as f32 / ASCII_CHARS.len() as f32;
    assert_eq!(out[0].uv, [u0, 0.0]);
}

#[test]
fn label_skips_characters_missing_from_the_atlas_but_keeps_their_space() {
    let label = PlotLabel {
        text: "a\u{3c3}b".to_string(),
        origin: [0.0, 0.0],
        scale: 1.0,
        color: [1.0; 4],
    };
    let mut out = Vec::new();
    push_label(&label, &mut out);

    assert_eq!(out.len(), 12, "the sigma is skipped");
    let b_left = out[6..]
        .iter()
        .map(|v| v.position[0])
        .fold(f32::MAX, f32::min);
    assert_eq!(b_left, 2.0 * GLYPH_CELL);
}

#[test]
fn plot_surface_prefers_a_format_without_srgb_encoding() {
    use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8UnormSrgb};

    assert_eq!(
        plot_surface_format(&[Bgra8UnormSrgb, Bgra8Unorm]),
        Some(Bgra8Unorm)
    );
    assert_eq!(plot_surface_format(&[Rgba8UnormSrgb]), Some(Rgba8UnormSrgb));
    assert_eq!(plot_surface_format(&[]), None);
}
