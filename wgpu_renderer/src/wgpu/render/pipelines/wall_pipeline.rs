// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wall rendering pipeline.
//!
//! Draws committed walls (and the wall ghost preview) as thick, anti-aliased
//! capsules using instanced quads. See `wall.wgsl`.

use bytemuck::{Pod, Zeroable};
use thermo_physics::application::AppResult;
use thermo_physics::objects2d::line_segment::THICKNESS_TO_WORLD;

use crate::wgpu::render::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::render::gpu::shaders;
use crate::wgpu::render::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

const WALL_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const WALL_PREVIEW_COLOR: [f32; 4] = [1.0, 1.0, 0.0, 0.5];

/// Per-wall GPU instance. Layout must match `Inst` in wall.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct WallInstance {
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    /// Full thickness in world units
    pub thickness: f32,
    pub color: [f32; 4],
}

const WALL_ATTRS: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Float32x4];

pub struct WallPipeline {
    gpu: GpuBatch<WallInstance>,
    /// Reused every frame to avoid reallocating
    instances: Vec<WallInstance>,
}

impl WallPipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Wall Pipeline", shaders::WALL, &WALL_ATTRS),
            ),
            instances: Vec::new(),
        }
    }
}

impl RenderPipeline for WallPipeline {
    fn name(&self) -> &'static str {
        "walls"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::new(ctx.view_proj, [0.0; 4]));

        self.instances.clear();

        // Committed walls
        for seg in &ctx.operators.line_segments {
            self.instances.push(WallInstance {
                p0: [seg.start.x, seg.start.y],
                p1: [seg.end.x, seg.end.y],
                thickness: THICKNESS_TO_WORLD * seg.thickness,
                color: WALL_COLOR,
            });
        }

        // Ghost wall while drawing a wall or dragging an endpoint, at its
        // real thickness: it already collides like the wall it will become.
        if let Some((start, end, thickness)) = ctx.operator_previews.line_segment {
            self.instances.push(WallInstance {
                p0: start,
                p1: end,
                thickness: THICKNESS_TO_WORLD * thickness,
                color: WALL_PREVIEW_COLOR,
            });
        }

        self.gpu.flush(ctx.device, ctx.queue, &self.instances);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}
