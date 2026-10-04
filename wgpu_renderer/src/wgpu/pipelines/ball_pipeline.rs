//! Ball rendering pipeline.
//!
//! One instanced quad per ball; the fragment shader draws the ball as a
//! shaded, anti-aliased SDF circle plus optional equilibrium / cutoff rings.
//! See `ball.wgsl`.

use bytemuck::{Pod, Zeroable};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::application::renderer_snapshot::BallRenderData;
use rust_thermo_core::application::scene_layers::SceneLayers;

use crate::wgpu::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::gpu::shaders;
use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Number of ring display modes (0 = equilibrium only, 1 = equilibrium +
/// cutoff, 2 = no rings).
const RING_DISPLAY_MODES: u32 = 3;

/// Per-ball GPU instance. Layout must match `Inst` in ball.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GpuBallData {
    /// Center position in world coordinates (x, y)
    pub position: [f32; 2],
    /// Ball radius in world units
    pub radius: f32,
    /// Packed RGBA color as u32 (0xAABBGGRR format, little-endian)
    pub color: u32,
}

impl GpuBallData {
    /// Create from render data, packing color into u32.
    pub fn from_render_data(data: &BallRenderData) -> Self {
        Self {
            position: [data.position.x, data.position.y],
            radius: data.radius,
            color: pack_color(data.color),
        }
    }
}

const BALL_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Uint32];

/// Pack RGBA [0.0-1.0] color into u32 (0xAABBGGRR format, little-endian).
fn pack_color(color: [f32; 4]) -> u32 {
    let r = (color[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (color[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (color[2].clamp(0.0, 1.0) * 255.0) as u32;
    let a = (color[3].clamp(0.0, 1.0) * 255.0) as u32;
    r | (g << 8) | (b << 16) | (a << 24)
}

pub struct BallPipeline {
    gpu: GpuBatch<GpuBallData>,
    /// Reused every frame to avoid reallocating
    instances: Vec<GpuBallData>,
    ring_display_mode: u32,
}

impl BallPipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Ball Pipeline", shaders::BALL, &BALL_ATTRS),
            ),
            instances: Vec::new(),
            ring_display_mode: 0,
        }
    }

    /// Cycle ring display mode; takes effect on the next prepared frame.
    pub fn cycle_ring_display_mode(&mut self) {
        self.ring_display_mode = (self.ring_display_mode + 1) % RING_DISPLAY_MODES;
    }
}

impl RenderPipeline for BallPipeline {
    fn name(&self) -> &'static str {
        "balls"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn layer(&self) -> SceneLayers {
        SceneLayers::BALLS
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let params = [
            ctx.snapshot.equilibrium_edge_distance,
            ctx.snapshot.cutoff_distance,
            self.ring_display_mode as f32,
            0.0,
        ];
        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::new(ctx.view_proj, params));

        self.instances.clear();
        self.instances
            .extend(ctx.snapshot.balls.iter().map(GpuBallData::from_render_data));
        self.gpu.flush(ctx.device, ctx.queue, &self.instances);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_color() {
        assert_eq!(pack_color([1.0, 1.0, 1.0, 1.0]), 0xFFFFFFFF);
        assert_eq!(pack_color([1.0, 0.0, 0.0, 1.0]), 0xFF0000FF);
        assert_eq!(pack_color([0.0, 1.0, 0.0, 1.0]), 0xFF00FF00);
        assert_eq!(pack_color([0.0, 0.0, 1.0, 1.0]), 0xFFFF0000);
        assert_eq!(pack_color([0.0, 0.0, 0.0, 0.0]), 0x00000000);
        // 0.5 * 255 = 127.5 -> 127 (truncation)
        assert_eq!(pack_color([0.5, 0.5, 0.5, 0.5]), 0x7F7F7F7F);
    }

    #[test]
    fn test_gpu_ball_data_size() {
        // position: vec2<f32> = 8, radius: f32 = 4, color: u32 = 4
        assert_eq!(std::mem::size_of::<GpuBallData>(), 16);
    }

    #[test]
    fn ring_display_mode_cycles_through_three_modes() {
        let mut mode = 0;
        for expected in [1, 2, 0, 1] {
            mode = (mode + 1) % RING_DISPLAY_MODES;
            assert_eq!(mode, expected);
        }
    }
}
