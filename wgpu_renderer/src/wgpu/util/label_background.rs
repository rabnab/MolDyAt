//! Semi-transparent white capsules drawn behind text labels.
//!
//! Shared by the thermostat and source-sphere label pipelines. A pipeline lays
//! out its text as usual, calls [`capsule_for_label`] on each label's glyph
//! vertices, then hands the resulting capsules to [`LabelBackground::update`].
//! Draw the background first and the text on top. See `label_capsule.wgsl`.

use crate::wgpu::instanced_gpu::InstancedGpu;
use crate::wgpu::world_text::TextVertex;
use bytemuck::{Pod, Zeroable};

/// Capsule padding around the text, in glyph widths (horizontal) and glyph
/// heights (vertical).
const CAPSULE_PAD_X_GLYPHS: f32 = 0.8;
const CAPSULE_PAD_Y_GLYPHS: f32 = 0.2;

/// One capsule, in NDC. Layout must match `Inst` in label_capsule.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct CapsuleInstance {
    center: [f32; 2],
    /// Half width / half height, padding included
    half_ext: [f32; 2],
}

const CAPSULE_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

/// Bounding box (min, max), in NDC, of the glyph quads in `verts`.
fn label_bounds(verts: &[TextVertex]) -> Option<([f32; 2], [f32; 2])> {
    // NOTE: assumes `TextVertex` has a `position: [f32; 2]` field in NDC.
    // Adjust this one line if the field is named or typed differently.
    let mut points = verts.iter().map(|v| v.position);
    let first = points.next()?;
    let (mut min, mut max) = (first, first);
    for p in points {
        min = [min[0].min(p[0]), min[1].min(p[1])];
        max = [max[0].max(p[0]), max[1].max(p[1])];
    }
    Some((min, max))
}

/// Capsule that fits the glyphs of ONE label (`label_verts` = just that label's
/// vertices), with padding scaled to the glyph size.
pub fn capsule_for_label(
    label_verts: &[TextVertex],
    glyph_ndc_w: f32,
    glyph_ndc_h: f32,
) -> Option<CapsuleInstance> {
    let (min, max) = label_bounds(label_verts)?;
    Some(CapsuleInstance {
        center: [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5],
        half_ext: [
            (max[0] - min[0]) * 0.5 + CAPSULE_PAD_X_GLYPHS * glyph_ndc_w.abs(),
            (max[1] - min[1]) * 0.5 + CAPSULE_PAD_Y_GLYPHS * glyph_ndc_h.abs(),
        ],
    })
}

/// GPU side of the label capsules.
pub struct LabelBackground {
    gpu: InstancedGpu<CapsuleInstance>,
}

impl LabelBackground {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let source = format!(
            "{}\n{}",
            include_str!("../shaders/common.wgsl"),
            include_str!("../shaders/label_capsule.wgsl"),
        );
        Self {
            // Positions are already NDC, so the default identity view_proj is right.
            gpu: InstancedGpu::new(device, format, "Label Capsule", &source, &CAPSULE_ATTRS),
        }
    }

    /// Upload this frame's capsules (pass an empty slice to draw none).
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        capsules: &[CapsuleInstance],
    ) {
        self.gpu.flush(device, queue, capsules);
    }

    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        self.gpu.draw(render_pass);
    }
}
