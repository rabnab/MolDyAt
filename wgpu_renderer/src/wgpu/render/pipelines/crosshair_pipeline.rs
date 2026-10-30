// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cursor crosshair pipeline.
//!
//! Draws a white crosshair with a dark outline at the cursor, at the system
//! cursor size in screen pixels whatever the world size (#386), and, while the
//! ball tool is active, the placement preview rings (ball, equilibrium and
//! cutoff radius). Rendered over the full window so it also shows over the
//! margins. See `solid_rect.wgsl` and `placement_rings.wgsl`.

use bytemuck::{Pod, Zeroable};
use thermo_physics::application::AppResult;

use crate::wgpu::render::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::render::gpu::geometry::mat4_mul;
use crate::wgpu::render::gpu::shaders;
use crate::wgpu::render::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Size of the crosshair in logical pixels when the system does not give
/// one: the usual X11/Wayland cursor size (`XCURSOR_SIZE`). It is drawn at
/// this size on screen whatever the world size or zoom (#386); the window's
/// scale factor turns it into physical pixels.
const DEFAULT_CURSOR_SIZE: f32 = 24.0;
/// Accepted range of a system cursor size, in logical pixels.
const CURSOR_SIZE_RANGE: std::ops::RangeInclusive<f32> = 8.0..=256.0;
/// Distance of each bar's centre from the cursor, as a fraction of the size.
const BAR_OFFSET: f32 = 0.3;
/// Bar length and thickness, as fractions of the size.
const BAR_LENGTH: f32 = 0.3;
const BAR_THICKNESS: f32 = 0.08;
/// Dark outline around each bar, as a fraction of the size.
const OUTLINE: f32 = 0.05;
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
    /// Crosshair size in logical pixels ([`system_cursor_size`]).
    cursor_size: f32,
}

/// The system's cursor size in logical pixels: `XCURSOR_SIZE` where it is
/// set to a sensible value, otherwise [`DEFAULT_CURSOR_SIZE`]. winit offers
/// no way to ask the window system.
fn system_cursor_size() -> f32 {
    cursor_size_from(std::env::var("XCURSOR_SIZE").ok().as_deref())
}

fn cursor_size_from(xcursor_size: Option<&str>) -> f32 {
    xcursor_size
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|size| CURSOR_SIZE_RANGE.contains(size))
        .unwrap_or(DEFAULT_CURSOR_SIZE)
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
            cursor_size: system_cursor_size(),
        }
    }
}

/// Four outlines first, then the four white bars on top. `size` is the
/// crosshair size in world units, so it is the same on screen at any world
/// scale ([`cursor_size_in_world`]).
fn crosshair_bars(cursor: [f32; 2], size: f32) -> [RectInstance; 8] {
    let (offset, length, thickness) = (BAR_OFFSET * size, BAR_LENGTH * size, BAR_THICKNESS * size);
    let horizontal = [length * 0.5, thickness * 0.5];
    let vertical = [thickness * 0.5, length * 0.5];
    let bars = [
        ([-offset, 0.0], horizontal),
        ([offset, 0.0], horizontal),
        ([0.0, offset], vertical),
        ([0.0, -offset], vertical),
    ];
    std::array::from_fn(|i| {
        let (offset, half) = bars[i % 4];
        let (grow, color) = if i < 4 {
            (OUTLINE * size, OUTLINE_COLOR)
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

/// `logical_size` logical pixels in world units: the window's scale factor
/// gives physical pixels, and the view projection (with zoom) maps world x
/// onto the drawing area of `area_width` physical pixels.
fn cursor_size_in_world(
    logical_size: f32,
    ui_scale: f32,
    view_proj: &[[f32; 4]; 4],
    area_width: f32,
) -> f32 {
    let px_per_world = view_proj[0][0].abs() * area_width * 0.5;
    if px_per_world > 0.0 && px_per_world.is_finite() {
        logical_size * ui_scale / px_per_world
    } else {
        0.0
    }
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
        let area_width = (ctx.window_size.0 as f32 - ctx.inset.horizontal()).max(1.0);
        let size = cursor_size_in_world(self.cursor_size, ctx.ui_scale, &ctx.view_proj, area_width);
        self.bars
            .flush(ctx.device, ctx.queue, &crosshair_bars(cursor, size));

        if ctx.show_ball_outline {
            let ball = ctx.current_ball_radius;
            let [eq, cutoff] = ctx.balls.cursor_ring_radii;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wgpu::render::gpu::geometry::build_view_proj_matrix;

    /// Width of the crosshair's outlined horizontal bars, outer edge to outer
    /// edge, in world units.
    fn crosshair_span(bars: &[RectInstance; 8]) -> f32 {
        let (left, right) = (bars[0], bars[1]);
        (right.center[0] + right.half_size[0]) - (left.center[0] - left.half_size[0])
    }

    fn span_in_pixels(world_width: f32, zoom: f32, ui_scale: f32) -> f32 {
        let area_width = 800.0;
        let view_proj = build_view_proj_matrix(world_width, world_width, 0.0, 0.0, zoom);
        let size = cursor_size_in_world(DEFAULT_CURSOR_SIZE, ui_scale, &view_proj, area_width);
        let px_per_world = view_proj[0][0].abs() * area_width * 0.5;
        crosshair_span(&crosshair_bars([0.0, 0.0], size)) * px_per_world
    }

    #[test]
    fn crosshair_keeps_its_screen_size_at_any_world_scale() {
        let small = span_in_pixels(100.0, 1.0, 1.0);
        let large = span_in_pixels(2000.0, 1.0, 1.0);
        let zoomed = span_in_pixels(2000.0, 3.0, 1.0);
        assert!((small - large).abs() < 1e-3, "{small} vs {large}");
        assert!((small - zoomed).abs() < 1e-3, "{small} vs {zoomed}");
        // Offset + half a bar + outline is half the size on each side.
        assert!((small - DEFAULT_CURSOR_SIZE).abs() < 1e-3, "{small}");
    }

    #[test]
    fn crosshair_follows_the_display_scale_factor() {
        let one = span_in_pixels(100.0, 1.0, 1.0);
        let two = span_in_pixels(100.0, 1.0, 2.0);
        assert!((two - 2.0 * one).abs() < 1e-3, "{one} vs {two}");
    }

    #[test]
    fn cursor_size_comes_from_xcursor_size_when_sensible() {
        assert_eq!(cursor_size_from(Some("32")), 32.0);
        assert_eq!(cursor_size_from(Some(" 48 ")), 48.0);
        assert_eq!(cursor_size_from(None), DEFAULT_CURSOR_SIZE);
        assert_eq!(cursor_size_from(Some("big")), DEFAULT_CURSOR_SIZE);
        assert_eq!(cursor_size_from(Some("0")), DEFAULT_CURSOR_SIZE);
        assert_eq!(cursor_size_from(Some("4096")), DEFAULT_CURSOR_SIZE);
    }
}
