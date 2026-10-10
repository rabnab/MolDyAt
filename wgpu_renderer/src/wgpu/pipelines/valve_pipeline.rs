// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Valve rendering pipeline.
//!
//! Draws each valve as an outlined rectangle laid over its wall:
//! blocking (closed) = dark red outline + diagonal stripes,
//! passable (open) = dark green outline, no fill. See `valve.wgsl`.

use bytemuck::{Pod, Zeroable};
use moldyat_core::application::AppResult;
use moldyat_core::objects2d::line_segment::valve_half_width;

use crate::wgpu::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::gpu::geometry::world_extent;
use crate::wgpu::gpu::shaders;
use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Outline width as a fraction of the world extent.
const VALVE_BORDER_FRAC: f32 = 0.003;

const STYLE_PASSABLE: f32 = 0.0;
const STYLE_BLOCKING: f32 = 1.0;
const STYLE_PREVIEW: f32 = 2.0;

/// Per-valve GPU instance. Layout must match `Inst` in valve.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct ValveInstance {
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub half_width: f32,
    pub border: f32,
    pub style: f32,
}

const VALVE_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Float32, 4 => Float32
];

pub struct ValvePipeline {
    gpu: GpuBatch<ValveInstance>,
    /// Reused every frame to avoid reallocating
    instances: Vec<ValveInstance>,
}

impl ValvePipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Valve Pipeline", shaders::VALVE, &VALVE_ATTRS),
            ),
            instances: Vec::new(),
        }
    }
}

impl RenderPipeline for ValvePipeline {
    fn name(&self) -> &'static str {
        "valves"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::new(ctx.view_proj, [0.0; 4]));

        let world_extent = world_extent(ctx.snapshot);
        let border = world_extent * VALVE_BORDER_FRAC;

        self.instances.clear();

        // Committed valves (already resolved to world space by the snapshot)
        for valve in &ctx.snapshot.operators.valve_spans {
            self.instances.push(ValveInstance {
                p0: [valve.p0.x, valve.p0.y],
                p1: [valve.p1.x, valve.p1.y],
                half_width: valve.half_width(world_extent),
                border,
                style: if valve.is_closed {
                    STYLE_BLOCKING
                } else {
                    STYLE_PASSABLE
                },
            });
        }

        // Ghost preview while drawing a valve: the tool reports piece-local
        // fractions on the wall piece being drawn on.
        if let Some((line_id, t_start, t_end)) = ctx.operator_previews.valve_segment
            && let Some(seg) = ctx
                .snapshot
                .operators
                .line_segments
                .iter()
                .find(|s| s.id == line_id)
        {
            let p0 = seg.point_at_t(t_start);
            let p1 = seg.point_at_t(t_end);
            self.instances.push(ValveInstance {
                p0: [p0.x, p0.y],
                p1: [p1.x, p1.y],
                half_width: valve_half_width(seg.thickness, world_extent),
                border,
                style: STYLE_PREVIEW,
            });
        }

        self.gpu.flush(ctx.device, ctx.queue, &self.instances);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}
