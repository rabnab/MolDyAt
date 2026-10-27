// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cursor crosshair pipeline.
//!
//! Draws a white crosshair with a dark outline at the cursor and, while the
//! ball tool is active, the placement preview rings (ball, equilibrium and
//! cutoff radius). Rendered over the full window so it also shows over the
//! margins. See `solid_rect.wgsl` and `placement_rings.wgsl`.

use bytemuck::{Pod, Zeroable};
use thermo_physics::application::AppResult;

use crate::wgpu::render::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::render::gpu::geometry::mat4_mul;
use crate::wgpu::render::gpu::shaders;
use crate::wgpu::render::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Distance of each bar's centre from the cursor (world units).
const BAR_OFFSET: f32 = 0.5;
/// Bar length and thickness (world units).
const BAR_LENGTH: f32 = 0.4;
const BAR_THICKNESS: f32 = 0.1;
/// Dark outline around each bar (world units).
const OUTLINE: f32 = 0.06;
const BAR_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const OUTLINE_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.85];

/// Rectangle instance. Layout must match `Inst` in solid_rect.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct RectInstance {
    pub center: [f32; 2],
    pub half_size: [f32; 2],
    pub color: [f32; 4],
}

/// Placement preview instance. Layout must match `Inst` in placement_rings.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct RingsInstance {
    pub center: [f32; 2],
    /// (ball radius, equilibrium radius, cutoff radius)
    pub radii: [f32; 3],
}

const RECT_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
const RINGS_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3];

pub struct CrosshairPipeline {
    bars: GpuBatch<RectInstance>,
    rings: GpuBatch<RingsInstance>,
}

impl CrosshairPipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            bars: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Crosshair Bars", shaders::SOLID_RECT, &RECT_ATTRS),
            ),
            rings: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Placement Rings", shaders::PLACEMENT_RINGS, &RINGS_ATTRS),
            ),
        }
    }
}

/// Four outlines first, then the four white bars on top.
fn crosshair_bars(cursor: [f32; 2]) -> [RectInstance; 8] {
    let horizontal = [BAR_LENGTH * 0.5, BAR_THICKNESS * 0.5];
    let vertical = [BAR_THICKNESS * 0.5, BAR_LENGTH * 0.5];
    let bars = [
        ([-BAR_OFFSET, 0.0], horizontal),
        ([BAR_OFFSET, 0.0], horizontal),
        ([0.0, BAR_OFFSET], vertical),
        ([0.0, -BAR_OFFSET], vertical),
    ];
    std::array::from_fn(|i| {
        let (offset, half) = bars[i % 4];
        let (grow, color) = if i < 4 {
            (OUTLINE, OUTLINE_COLOR)
        } else {
            (0.0, BAR_COLOR)
        };
        RectInstance {
            center: [cursor[0] + offset[0], cursor[1] + offset[1]],
            half_size: [half[0] + grow, half[1] + grow],
            color,
        }
    })
}

/// View-projection for the full-window viewport: the inset `view_proj`
/// followed by the inset-NDC → window-NDC remap.
fn full_window_view_proj(ctx: &PrepareContext) -> [[f32; 4]; 4] {
    let (fw, fh) = (ctx.window_size.0 as f32, ctx.window_size.1 as f32);
    let inset = ctx.inset;
    let sx = (fw - inset.horizontal()).max(1.0) / fw;
    let sy = (fh - inset.vertical()).max(1.0) / fh;
    let ox = (inset.left - inset.right) / fw;
    let oy = (inset.bottom - inset.top) / fh;
    let remap = [
        [sx, 0.0, 0.0, 0.0],
        [0.0, sy, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [ox, oy, 0.0, 1.0],
    ];
    mat4_mul(&remap, &ctx.view_proj)
}

impl RenderPipeline for CrosshairPipeline {
    fn name(&self) -> &'static str {
        "crosshair"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::FullWindow
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let uniforms = SceneUniforms::new(full_window_view_proj(ctx), [0.0; 4]);
        self.bars.set_uniforms(ctx.queue, uniforms);
        self.rings.set_uniforms(ctx.queue, uniforms);

        let cursor = [ctx.cursor_world_pos.0, ctx.cursor_world_pos.1];
        self.bars
            .flush(ctx.device, ctx.queue, &crosshair_bars(cursor));

        if ctx.show_ball_outline {
            let ball = ctx.current_ball_radius;
            let [eq, cutoff] = ctx.snapshot.cursor_ring_radii;
            let rings = RingsInstance {
                center: cursor,
                radii: [ball, eq, cutoff],
            };
            self.rings.flush(ctx.device, ctx.queue, &[rings]);
        } else {
            self.rings.clear();
        }
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.bars.draw(render_pass);
        self.rings.draw(render_pass);
    }
}
