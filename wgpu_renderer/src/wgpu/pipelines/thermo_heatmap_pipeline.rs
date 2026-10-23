// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Thermo heatmap overlay pipeline.
//!
//! Draws one quad covering the simulation world area. The scalar T/P field is
//! uploaded as an `R8Unorm` texture (one texel per cell) and reconstructed in
//! the fragment shader with a cubic B-spline filter, so a coarse grid looks
//! like a smooth surface. See `thermo_heatmap.wgsl`.
//!
//! To keep the picture calm the field is smoothed before upload:
//!   1. NaN cells are filled from their neighbours,
//!   2. a small spatial blur removes cell-to-cell noise,
//!   3. an exponential moving average over time removes frame-to-frame jiggle.
//!
//! Encoding (R8Unorm): the field value normalised to `[0, 1]` maps to bytes
//! `0..=255`. There is no "no data" sentinel: NaN cells are filled first, and an
//! all-NaN field simply isn't drawn.

use bytemuck::{Pod, Zeroable};
use std::time::Instant;
use thermo_physics::application::AppResult;
use thermo_physics::application::thermo_field::ThermoGrid;
use thermo_runtime::application::renderer_snapshot::ThermoGridSpec;
use thermo_runtime::application::scene_layers::SceneLayers;
use thermo_ui::graphics::renderer::interpolation::fill_nans_with_neighbour_mean;
use thermo_ui::graphics::renderer::state_layers::ThermoDisplayMode;

use crate::wgpu::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::gpu::shaders;
use crate::wgpu::gpu::textures::{r8_texture, write_r8};
use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

// Fixed normalisation ranges of the displayed field.
const TEMP_MIN: f32 = 0.0;
const TEMP_MAX: f32 = 5.0;
const PRES_MIN: f32 = -4.0;
const PRES_MAX: f32 = 4.0;

/// Number of 3x3 spatial blur passes (weights 1-2-1 per axis) applied to the
/// field before upload. 0 = off, 1 = gentle, 2 = strong.
const SPATIAL_BLUR_PASSES: usize = 1;
/// Time constant (seconds) of the moving average applied to the displayed
/// field. Larger = calmer but slower to follow real changes.
const TEMPORAL_SMOOTHING_SECS: f32 = 0.75;

const TEXTURE_LABEL: &str = "Thermo Heatmap Texture";

/// Heatmap GPU instance. Layout must match `Inst` in thermo_heatmap.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct HeatmapInstance {
    /// World-space bottom-left corner
    origin: [f32; 2],
    /// World-space (width, height)
    size: [f32; 2],
}

const HEATMAP_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

/// Time-smoothed field currently on screen.
struct TemporalSmoother {
    /// Row-major, all finite
    shown: Vec<f32>,
    dims: (usize, usize),
    /// Display mode `shown` belongs to (temperature vs pressure).
    mode: Option<ThermoDisplayMode>,
    last: Instant,
}

pub struct ThermoHeatmapPipeline {
    gpu: GpuBatch<HeatmapInstance>,
    texture: wgpu::Texture,
    /// (cols, rows) of `texture`
    texture_dims: (u32, u32),
    smoother: TemporalSmoother,
}

impl ThermoHeatmapPipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        // 1×1 placeholder; replaced when the first field arrives.
        let (texture, view) = r8_texture(device, TEXTURE_LABEL, 1, 1);
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Thermo Heatmap", shaders::THERMO_HEATMAP, &HEATMAP_ATTRS)
                    .texture(&view, wgpu::FilterMode::Linear),
            ),
            texture,
            texture_dims: (1, 1),
            smoother: TemporalSmoother {
                shown: Vec::new(),
                dims: (0, 0),
                mode: None,
                last: Instant::now(),
            },
        }
    }

    /// Resize the field texture if the grid changed shape.
    fn ensure_texture(&mut self, device: &wgpu::Device, cols: u32, rows: u32) {
        if self.texture_dims == (cols, rows) {
            return;
        }
        let (texture, view) = r8_texture(device, TEXTURE_LABEL, cols, rows);
        self.gpu.set_texture(device, &view);
        self.texture = texture;
        self.texture_dims = (cols, rows);
    }

    /// Smooth, normalise and upload the field; returns `false` when there is
    /// nothing to draw (empty or all-NaN field).
    fn upload_field(
        &mut self,
        ctx: &PrepareContext,
        grid: &ThermoGrid,
        spec: &ThermoGridSpec,
        mode: ThermoDisplayMode,
        (v_min, v_max): (f32, f32),
    ) -> bool {
        let (rows, cols) = grid.shape();
        if rows == 0 || cols == 0 {
            return false;
        }

        let mut field = fill_nans_with_neighbour_mean(grid.as_slice(), rows, cols);
        if field.iter().all(|v| !v.is_finite()) {
            return false;
        }
        for _ in 0..SPATIAL_BLUR_PASSES {
            field = blur_3x3(&field, rows, cols);
        }
        self.smoother.smooth(&mut field, rows, cols, mode);

        // Row 0 of ThermoGrid = world bottom → texture row 0 (V = 0).
        let inv_range = 1.0 / (v_max - v_min);
        let bytes: Vec<u8> = field
            .iter()
            .map(|&v| (((v - v_min) * inv_range).clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect();

        let (cols, rows) = (cols as u32, rows as u32);
        self.ensure_texture(ctx.device, cols, rows);
        write_r8(ctx.queue, &self.texture, &bytes, cols, rows);

        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::new(ctx.view_proj, [0.0; 4]));
        let quad = HeatmapInstance {
            origin: [spec.origin_x, spec.origin_y],
            size: [
                spec.cell_width * cols as f32,
                spec.cell_height * rows as f32,
            ],
        };
        self.gpu.flush(ctx.device, ctx.queue, &[quad]);
        true
    }
}

impl TemporalSmoother {
    /// Exponential moving average over time, in place.
    ///
    /// Restarts from the current field when the grid shape or the display mode
    /// (temperature / pressure) changes. The weight is derived from the elapsed
    /// time, so the smoothing doesn't depend on the frame rate, and after a long
    /// pause (e.g. the overlay was switched off) the field snaps to the current
    /// values instead of sliding from stale ones.
    fn smooth(&mut self, field: &mut [f32], rows: usize, cols: usize, mode: ThermoDisplayMode) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32();
        self.last = now;

        let restart =
            self.shown.len() != field.len() || self.dims != (rows, cols) || self.mode != Some(mode);
        if restart {
            self.shown.clear();
            self.shown.extend_from_slice(field);
            self.dims = (rows, cols);
            self.mode = Some(mode);
            return;
        }

        let alpha = 1.0 - (-dt / TEMPORAL_SMOOTHING_SECS).exp();
        for (shown, &target) in self.shown.iter_mut().zip(field.iter()) {
            *shown += (target - *shown) * alpha;
        }
        field.copy_from_slice(&self.shown);
    }
}

/// One separable 3x3 blur pass with weights 1-2-1 per axis. Edges replicate
/// the border cell, so the field isn't pulled towards zero at the boundary.
fn blur_3x3(data: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    if rows == 0 || cols == 0 {
        return data.to_vec();
    }

    // Horizontal pass
    let mut tmp = vec![0.0_f32; data.len()];
    for r in 0..rows {
        for c in 0..cols {
            let left = data[r * cols + c.saturating_sub(1)];
            let mid = data[r * cols + c];
            let right = data[r * cols + (c + 1).min(cols - 1)];
            tmp[r * cols + c] = 0.25 * left + 0.5 * mid + 0.25 * right;
        }
    }

    // Vertical pass
    let mut out = vec![0.0_f32; data.len()];
    for r in 0..rows {
        let up = r.saturating_sub(1);
        let down = (r + 1).min(rows - 1);
        for c in 0..cols {
            out[r * cols + c] =
                0.25 * tmp[up * cols + c] + 0.5 * tmp[r * cols + c] + 0.25 * tmp[down * cols + c];
        }
    }
    out
}

impl RenderPipeline for ThermoHeatmapPipeline {
    fn name(&self) -> &'static str {
        "thermo_heatmap"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn layer(&self) -> SceneLayers {
        SceneLayers::THERMO
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let mode = ctx.thermo_display;
        let (grid, range) = match mode {
            ThermoDisplayMode::Temperature => {
                (&ctx.snapshot.cell_temperature, (TEMP_MIN, TEMP_MAX))
            }
            ThermoDisplayMode::Pressure => (&ctx.snapshot.cell_pressure, (PRES_MIN, PRES_MAX)),
            ThermoDisplayMode::Off | ThermoDisplayMode::Grid => {
                self.gpu.clear();
                return Ok(());
            }
        };
        let Some(spec) = &ctx.snapshot.thermo_grid_spec else {
            log::debug!("ThermoHeatmapPipeline: no thermo_grid_spec, skipping");
            self.gpu.clear();
            return Ok(());
        };

        if !self.upload_field(ctx, grid, spec, mode, range) {
            self.gpu.clear();
        }
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}
