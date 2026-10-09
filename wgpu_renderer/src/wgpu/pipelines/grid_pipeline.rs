// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cell grid rendering pipeline.
//!
//! Draws the spatial cell grid as one instanced quad covering the whole grid;
//! the fragment shader colours each cell and the border. Visible only in
//! `ThermoDisplayMode::Grid`. See `grid.wgsl`.

use bytemuck::{Pod, Zeroable};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::graphics::renderer::state_layers::ThermoDisplayMode;

use crate::wgpu::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::gpu::shaders;
use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Grid GPU instance. Layout must match `Inst` in grid.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GridInstance {
    /// Top-left corner in world coordinates
    pub origin: [f32; 2],
    pub cell_size: [f32; 2],
    /// (cols, rows)
    pub dims: [f32; 2],
}

const GRID_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2];

pub struct GridPipeline {
    gpu: GpuBatch<GridInstance>,
}

impl GridPipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Grid Pipeline", shaders::GRID, &GRID_ATTRS),
            ),
        }
    }
}

impl RenderPipeline for GridPipeline {
    fn name(&self) -> &'static str {
        "grid"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let spec = &ctx.snapshot.grid_spec;
        if ctx.thermo_display != ThermoDisplayMode::Grid || spec.cols == 0 || spec.rows == 0 {
            self.gpu.clear();
            return Ok(());
        }

        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::new(ctx.view_proj, [0.0; 4]));
        let grid = GridInstance {
            origin: [spec.origin_x, spec.origin_y],
            cell_size: [spec.cell_width, spec.cell_height],
            dims: [spec.cols as f32, spec.rows as f32],
        };
        self.gpu.flush(ctx.device, ctx.queue, &[grid]);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}
