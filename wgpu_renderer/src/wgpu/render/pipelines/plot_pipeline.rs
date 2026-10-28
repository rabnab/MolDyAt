// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Energy plot pipeline: draws a [`PlotScene`] (laid out by `thermo_ui`) into
//! the energy plot window, with anti-aliased line segments
//! (`plot_segment.wgsl`) and coloured bitmap text (`plot_text.wgsl`).
//!
//! It is not part of the main window's render graph: the plot window has its
//! own surface (see `plot_surface.rs`) and draws a scene, not a snapshot.

use bytemuck::{Pod, Zeroable};
use thermo_ui::graphics::energy_plot::plot_layout::GLYPH_CELL;
use thermo_ui::graphics::energy_plot::{PlotLabel, PlotScene};

use crate::wgpu::render::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::render::gpu::shaders;
use crate::wgpu::render::gpu::text::{ASCII_CHARS, GlyphAtlas, ascii_index};

/// One segment. Layout must match `Inst` in plot_segment.wgsl.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct SegmentInstance {
    /// Window pixels
    pub(crate) p0: [f32; 2],
    pub(crate) p1: [f32; 2],
    pub(crate) half_width: f32,
    pub(crate) color: [f32; 4],
}

const SEGMENT_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Float32,
    3 => Float32x4,
];

/// One glyph-quad corner. Layout must match `Vertex` in plot_text.wgsl.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct PlotTextVertex {
    /// Window pixels
    pub(crate) position: [f32; 2],
    pub(crate) uv: [f32; 2],
    pub(crate) color: [f32; 4],
}

const TEXT_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

/// The segments of `scene` as instances, appended to `out`.
pub(crate) fn segment_instances(scene: &PlotScene, out: &mut Vec<SegmentInstance>) {
    out.extend(scene.segments.iter().map(|s| SegmentInstance {
        p0: s.from,
        p1: s.to,
        half_width: s.width * 0.5,
        color: s.color,
    }));
}

/// Six vertices per glyph of `label`, appended to `out`. Characters the
/// printable-ASCII atlas lacks are skipped but keep their space.
pub(crate) fn push_label(label: &PlotLabel, out: &mut Vec<PlotTextVertex>) {
    let glyph = GLYPH_CELL * label.scale;
    let atlas_len = ASCII_CHARS.len() as f32;
    let [x0, top] = label.origin;
    let bottom = top + glyph;
    for (i, ch) in label.text.chars().enumerate() {
        let Some(index) = ascii_index(ch) else {
            continue;
        };
        let (u0, u1) = (index as f32 / atlas_len, (index + 1) as f32 / atlas_len);
        let left = x0 + i as f32 * glyph;
        let right = left + glyph;
        let vertex = |position, uv| PlotTextVertex {
            position,
            uv,
            color: label.color,
        };
        let tl = vertex([left, top], [u0, 0.0]);
        let bl = vertex([left, bottom], [u0, 1.0]);
        let tr = vertex([right, top], [u1, 0.0]);
        let br = vertex([right, bottom], [u1, 1.0]);
        out.extend_from_slice(&[tl, bl, tr, bl, br, tr]);
    }
}

pub(crate) struct PlotPipeline {
    segments: GpuBatch<SegmentInstance>,
    text: GpuBatch<PlotTextVertex>,
    /// Reused every frame to avoid reallocating
    segment_scratch: Vec<SegmentInstance>,
    text_scratch: Vec<PlotTextVertex>,
}

impl PlotPipeline {
    /// The text is drawn from the renderer's shared `glyphs` (#303).
    pub(crate) fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        glyphs: &GlyphAtlas,
    ) -> Self {
        Self {
            segments: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Plot Segments", shaders::PLOT_SEGMENT, &SEGMENT_ATTRS),
            ),
            text: GpuBatch::new(
                device,
                format,
                BatchDesc::vertices(
                    "Plot Text",
                    shaders::PLOT_TEXT,
                    &TEXT_ATTRS,
                    wgpu::PrimitiveTopology::TriangleList,
                )
                // Nearest keeps the 8×8 bitmap glyphs crisp.
                .texture(glyphs.view(), wgpu::FilterMode::Nearest),
            ),
            segment_scratch: Vec::new(),
            text_scratch: Vec::new(),
        }
    }

    /// Upload `scene` for the next [`Self::render`].
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &PlotScene,
    ) {
        let size = [scene.size.0 as f32, scene.size.1 as f32, 0.0, 0.0];
        self.segments.set_uniforms(queue, SceneUniforms::ndc(size));
        self.text.set_uniforms(queue, SceneUniforms::ndc(size));

        self.segment_scratch.clear();
        segment_instances(scene, &mut self.segment_scratch);
        self.segments.flush(device, queue, &self.segment_scratch);

        self.text_scratch.clear();
        for label in &scene.labels {
            push_label(label, &mut self.text_scratch);
        }
        self.text.flush(device, queue, &self.text_scratch);
    }

    /// Lines first, text over them.
    pub(crate) fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.segments.draw(render_pass);
        self.text.draw(render_pass);
    }
}
