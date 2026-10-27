// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Hairline rendering pipeline.
//!
//! Draws 1-pixel world-space lines (LineList with per-vertex colour):
//! neighbour lines, the source-sphere speed/rate arcs and snap indicators.
//! See `line.wgsl`.

use bytemuck::{Pod, Zeroable};
use std::f32::consts::{FRAC_PI_2, TAU};
use thermo_physics::application::AppResult;
use thermo_physics::objects2d::SimSourceSphere;
use thermo_runtime::application::scene_layers::SceneLayers;
use thermo_ui::graphics::renderer::batch_renderer::{
    BatchRenderer, LineSegment, collect_neighbor_lines,
};
use thermo_ui::graphics::renderer::event_loop::DEFAULT_SOURCE_RADIUS;

use crate::wgpu::render::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::render::gpu::geometry::world_extent;
use crate::wgpu::render::gpu::shaders;
use crate::wgpu::render::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Line segments per full circle for the source-sphere arcs.
const ARC_SEGMENTS: usize = 32;
const SPEED_ARC_COLOR: [f32; 4] = [0.1, 0.4, 1.0, 0.9]; // blue
const RATE_ARC_COLOR: [f32; 4] = [0.05, 0.05, 0.05, 0.85]; // near-black
const SNAP_COLOR: [f32; 4] = [0.0, 1.0, 1.0, 0.9]; // cyan
/// Snap crosshair half-length as a fraction of the world extent.
const SNAP_CROSS_FRAC: f32 = 0.015;

/// GPU line vertex. Layout must match `Vertex` in line.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct LineVertex {
    /// World coordinates
    pub position: [f32; 2],
    /// RGBA (0.0-1.0)
    pub color: [f32; 4],
}

const LINE_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

pub struct LinePipeline {
    gpu: GpuBatch<LineVertex>,
    /// Reused every frame to avoid reallocating
    batch: BatchRenderer<LineSegment>,
    vertices: Vec<LineVertex>,
}

impl LinePipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::vertices(
                    "Line Pipeline",
                    shaders::LINE,
                    &LINE_ATTRS,
                    wgpu::PrimitiveTopology::LineList,
                ),
            ),
            batch: BatchRenderer::new(),
            vertices: Vec::new(),
        }
    }
}

/// Arc of `fraction` of a full circle, starting at 12 o'clock and running
/// counter-clockwise (`direction` = 1) or clockwise (`direction` = -1).
fn push_arc(
    batch: &mut BatchRenderer<LineSegment>,
    center: [f32; 2],
    radius: f32,
    fraction: f32,
    direction: f32,
    color: [f32; 4],
) {
    if fraction <= 0.0 {
        return;
    }
    let sweep = fraction * TAU;
    let n = ((ARC_SEGMENTS as f32 * fraction).ceil() as usize).max(1);
    let point = |i: usize| {
        let a = -FRAC_PI_2 - direction * (i as f32 / n as f32) * sweep;
        [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
    };
    for i in 0..n {
        batch.push(LineSegment {
            points: [point(i), point(i + 1)],
            width: 2.0,
            color,
        });
    }
}

/// Inner arc (blue, CCW): emission speed. Outer arc (black, CW): emission
/// rate. Both sit outside the icon quad.
fn push_source_arcs(batch: &mut BatchRenderer<LineSegment>, sphere: &SimSourceSphere) {
    let center = [sphere.center.x, sphere.center.y];
    let inner = DEFAULT_SOURCE_RADIUS * 1.15;
    let outer = DEFAULT_SOURCE_RADIUS * 1.35;
    push_arc(
        batch,
        center,
        inner,
        sphere.speed_fraction(),
        1.0,
        SPEED_ARC_COLOR,
    );
    push_arc(
        batch,
        center,
        outer,
        sphere.rate_fraction(),
        -1.0,
        RATE_ARC_COLOR,
    );
}

/// Small cross at a snap target position.
fn push_snap_cross(batch: &mut BatchRenderer<LineSegment>, pos: [f32; 2], half: f32) {
    for (dx, dy) in [(half, 0.0), (0.0, half)] {
        batch.push(LineSegment {
            points: [[pos[0] - dx, pos[1] - dy], [pos[0] + dx, pos[1] + dy]],
            width: 2.0,
            color: SNAP_COLOR,
        });
    }
}

impl RenderPipeline for LinePipeline {
    fn name(&self) -> &'static str {
        "lines"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::new(ctx.view_proj, [0.0; 4]));

        // Neighbour lines belong to the balls; operator arcs and snap
        // crosses are drawn in every frame.
        if ctx.layers.contains(SceneLayers::BALLS) {
            collect_neighbor_lines(&ctx.snapshot.balls, &mut self.batch);
        }
        for sphere in &ctx.snapshot.operators.source_spheres {
            push_source_arcs(&mut self.batch, sphere);
        }
        let cross_half = world_extent(ctx.snapshot) * SNAP_CROSS_FRAC;
        for &pos in &ctx.operator_previews.snap_indicators {
            push_snap_cross(&mut self.batch, pos, cross_half);
        }

        self.vertices.clear();
        for seg in self.batch.drain() {
            self.vertices.extend(seg.points.map(|position| LineVertex {
                position,
                color: seg.color,
            }));
        }
        self.gpu.flush(ctx.device, ctx.queue, &self.vertices);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}
