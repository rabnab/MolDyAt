// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Flow arrows (#342): one arrow per thermo cell along the cell's
//! centre-of-mass velocity, drawn as hairlines with `line.wgsl`. Arrow
//! lengths are relative to the fastest cell, so the picture shows the
//! flow pattern at any speed. Shown in the `FlowArrows` overlay mode.

use thermo_physics::application::AppResult;
use thermo_physics::application::thermo_field::CellFields;
use thermo_runtime::application::renderer_snapshot::ThermoGridSpec;
use thermo_runtime::application::scene_layers::SceneLayers;
use thermo_ui::graphics::renderer::state_layers::ThermoDisplayMode;

use crate::wgpu::render::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::render::gpu::shaders;
use crate::wgpu::render::pipelines::line_pipeline::{LINE_ATTRS, LineVertex};
use crate::wgpu::render::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

const ARROW_COLOR: [f32; 4] = [1.0, 0.85, 0.2, 0.95]; // amber
/// Length of the fastest cell's arrow as a fraction of the smaller cell side.
const MAX_LENGTH_FRAC: f32 = 0.8;
/// Length of each head stroke as a fraction of the arrow's length.
const HEAD_LENGTH_FRAC: f32 = 0.3;
/// Angle between the shaft and each head stroke (radians, about 25°).
const HEAD_ANGLE: f32 = 0.436;

pub struct FlowArrowPipeline {
    gpu: GpuBatch<LineVertex>,
    vertices: Vec<LineVertex>,
}

impl FlowArrowPipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::vertices(
                    "Flow Arrows",
                    shaders::LINE,
                    &LINE_ATTRS,
                    wgpu::PrimitiveTopology::LineList,
                ),
            ),
            vertices: Vec::new(),
        }
    }
}

/// Line-list vertices (world space) of one arrow per cell with a finite,
/// non-zero flow velocity, centred on the cell, into `out`.
pub(crate) fn flow_arrow_vertices(
    cells: &CellFields,
    spec: &ThermoGridSpec,
    out: &mut Vec<LineVertex>,
) {
    out.clear();
    let speeds = cells.flow_speed();
    let max_speed = speeds
        .as_slice()
        .iter()
        .copied()
        .filter(|s| s.is_finite())
        .fold(0.0_f32, f32::max);
    if max_speed <= 0.0 {
        return;
    }
    let max_length = MAX_LENGTH_FRAC * spec.cell_width.min(spec.cell_height);
    let (rows, cols) = cells.shape();
    let vertex = |x: f32, y: f32| LineVertex {
        position: [x, y],
        color: ARROW_COLOR,
    };
    for r in 0..rows {
        for c in 0..cols {
            let speed = speeds.get(r, c);
            if !speed.is_finite() || speed <= 0.0 {
                continue;
            }
            let (dx, dy) = (
                cells.velocity_x.get(r, c) / speed,
                cells.velocity_y.get(r, c) / speed,
            );
            let half = 0.5 * max_length * speed / max_speed;
            let cx = spec.origin_x + (c as f32 + 0.5) * spec.cell_width;
            let cy = spec.origin_y + (r as f32 + 0.5) * spec.cell_height;
            let tip = (cx + dx * half, cy + dy * half);
            out.push(vertex(cx - dx * half, cy - dy * half));
            out.push(vertex(tip.0, tip.1));
            let head = HEAD_LENGTH_FRAC * 2.0 * half;
            for angle in [HEAD_ANGLE, -HEAD_ANGLE] {
                // The shaft direction turned back by 180° ± the head angle.
                let (sin, cos) = angle.sin_cos();
                let (bx, by) = (-(dx * cos - dy * sin), -(dx * sin + dy * cos));
                out.push(vertex(tip.0, tip.1));
                out.push(vertex(tip.0 + bx * head, tip.1 + by * head));
            }
        }
    }
}

impl RenderPipeline for FlowArrowPipeline {
    fn name(&self) -> &'static str {
        "flow_arrows"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn layer(&self) -> SceneLayers {
        SceneLayers::THERMO
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let spec = ctx.fields.thermo_grid.as_ref();
        match spec {
            Some(spec) if ctx.thermo_display == ThermoDisplayMode::FlowArrows => {
                flow_arrow_vertices(&ctx.fields.cells, spec, &mut self.vertices);
            }
            _ => self.vertices.clear(),
        }
        if self.vertices.is_empty() {
            self.gpu.clear();
            return Ok(());
        }
        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::new(ctx.view_proj, [0.0; 4]));
        self.gpu.flush(ctx.device, ctx.queue, &self.vertices);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}
