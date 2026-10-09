// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Hover highlight pipeline.
//!
//! Draws a glowing outline around the object the active tool reports under
//! the cursor (`FramePreviews::hovered`): a whole wall including its valves,
//! a single valve, a thermostat or a source sphere. Every shape is a capsule
//! (a circle is a zero-length one). See `highlight.wgsl`.

use bytemuck::{Pod, Zeroable};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::application::renderer_snapshot::OperatorSnapshot;
use rust_thermo_core::objects2d::line_segment::THICKNESS_TO_WORLD;
use rust_thermo_core::objects2d::wall_chain::ObjectRef;
use rust_thermo_core::objects2d::{SOURCE_SPHERE_RADIUS, Vec2D};

use crate::wgpu::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::gpu::geometry::world_extent;
use crate::wgpu::gpu::shaders;
use crate::wgpu::pipelines::thermostat_pipeline::ICON_HALF_SIZE_FRAC;
use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Outline band width as a fraction of the world extent.
const BAND_WIDTH_FRAC: f32 = 0.003;

/// Per-highlight GPU instance. Layout must match `Inst` in highlight.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct HighlightInstance {
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    /// Capsule radius (world units): the outline starts here.
    pub radius: f32,
    /// Outline band width (world units).
    pub width: f32,
}

const HIGHLIGHT_ATTRS: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Float32];

/// The capsule `(p0, p1, radius)` enclosing `target`, or `None` if it is no
/// longer in the snapshot (e.g. deleted a moment ago).
pub(crate) fn highlight_capsule(
    ops: &OperatorSnapshot,
    target: ObjectRef,
    world_extent: f32,
) -> Option<(Vec2D, Vec2D, f32)> {
    match target {
        ObjectRef::Wall(chain_id) => {
            let wall = ops.walls.iter().find(|w| w.chain_id == chain_id)?;
            let piece_radius = ops
                .line_segments
                .iter()
                .filter(|seg| wall.piece_ids.contains(&seg.id))
                .map(|seg| 0.5 * seg.thickness * THICKNESS_TO_WORLD);
            let valve_radius = ops
                .valve_spans
                .iter()
                .filter(|v| wall.valve_ids.contains(&v.valve_id))
                .map(|v| v.half_width(world_extent));
            // One capsule over the whole wall, as wide as its widest piece or
            // valve: simple and gap-free, but it over-covers thinner parts of
            // a wall whose pieces have different thicknesses.
            let radius = piece_radius.chain(valve_radius).fold(0.0, f32::max);
            Some((wall.start, wall.end, radius))
        }
        ObjectRef::Valve(valve_id) => {
            let v = ops.valve_spans.iter().find(|v| v.valve_id == valve_id)?;
            Some((v.p0, v.p1, v.half_width(world_extent)))
        }
        ObjectRef::Thermostat(id) => {
            let t = ops.thermostats.iter().find(|t| t.id == id)?;
            Some((t.position, t.position, world_extent * ICON_HALF_SIZE_FRAC))
        }
        ObjectRef::SourceSphere(id) => {
            let s = ops.source_spheres.iter().find(|s| s.id == id)?;
            Some((s.center, s.center, SOURCE_SPHERE_RADIUS))
        }
    }
}

pub struct HighlightPipeline {
    gpu: GpuBatch<HighlightInstance>,
    /// Reused every frame to avoid reallocating
    instances: Vec<HighlightInstance>,
}

impl HighlightPipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Highlight Pipeline", shaders::HIGHLIGHT, &HIGHLIGHT_ATTRS),
            ),
            instances: Vec::new(),
        }
    }
}

impl RenderPipeline for HighlightPipeline {
    fn name(&self) -> &'static str {
        "highlight"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::new(ctx.view_proj, [0.0; 4]));

        let extent = world_extent(ctx.snapshot);
        self.instances.clear();
        if let Some(target) = ctx.operator_previews.hovered
            && let Some((p0, p1, radius)) =
                highlight_capsule(&ctx.snapshot.operators, target, extent)
        {
            self.instances.push(HighlightInstance {
                p0: [p0.x, p0.y],
                p1: [p1.x, p1.y],
                radius,
                width: extent * BAND_WIDTH_FRAC,
            });
        }

        self.gpu.flush(ctx.device, ctx.queue, &self.instances);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}
