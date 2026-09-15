//! Wall rendering pipeline.
//!
//! Draws committed walls (and the wall ghost preview) as thick, anti-aliased
//! capsules using instanced quads. See `wall.wgsl`.

use super::super::instanced_gpu::InstancedGpu;
use super::super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use bytemuck::{Pod, Zeroable};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::objects2d::line_segment::THICKNESS_TO_WORLD;

/// Wall thickness as a fraction of the world extent (tune to taste).
const WALL_THICKNESS_FRAC: f32 = 0.006;
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
    gpu: InstancedGpu<WallInstance>,
    /// Reused every frame to avoid reallocating
    instances: Vec<WallInstance>,
}

impl WallPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        // WGSL has no #include: prepend the shared header at startup.
        let source = format!(
            "{}\n{}",
            include_str!("../shaders/common.wgsl"),
            include_str!("../shaders/wall.wgsl"),
        );
        Self {
            gpu: InstancedGpu::new(device, format, "Wall Pipeline", &source, &WALL_ATTRS),
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
        self.gpu.update_uniforms(ctx.queue, ctx.view_proj, 0.0);

        self.instances.clear();

        // Committed walls
        for seg in &ctx.snapshot.operators.line_segments {
            self.instances.push(WallInstance {
                p0: [seg.start.x, seg.start.y],
                p1: [seg.end.x, seg.end.y],
                thickness: THICKNESS_TO_WORLD * seg.thickness,
                color: WALL_COLOR,
            });
        }

        // Ghost preview while drawing a wall
        if let Some((start, end)) = ctx.operator_previews.line_segment {
            let bounds = &ctx.snapshot.cell_grid_bounds;
            let world_extent = bounds.w.abs().max(bounds.h.abs()).max(1.0);
            let thickness = world_extent * WALL_THICKNESS_FRAC;

            self.instances.push(WallInstance {
                p0: start,
                p1: end,
                thickness,
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
