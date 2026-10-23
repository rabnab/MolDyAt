// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Frame border pipeline: a thin outline around the inset simulation
//! viewport, drawn in NDC over the full window. See `frame.wgsl`.

use bytemuck::{Pod, Zeroable};
use thermo_physics::application::AppResult;
use thermo_ui::graphics::renderer::margins::Margins;

use crate::wgpu::gpu::batch::{BatchDesc, GpuBatch};
use crate::wgpu::gpu::geometry::px_to_ndc;
use crate::wgpu::gpu::shaders;
use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Layout must match the vertex input of frame.wgsl.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct FrameVertex {
    /// NDC
    position: [f32; 2],
}

const FRAME_ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];

pub struct FramePipeline {
    gpu: GpuBatch<FrameVertex>,
}

impl FramePipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::vertices(
                    "Frame Pipeline",
                    shaders::FRAME,
                    &FRAME_ATTRS,
                    wgpu::PrimitiveTopology::LineStrip,
                ),
            ),
        }
    }
}

/// Closed outline (5 vertices) of the inset viewport, in NDC.
fn frame_vertices(width: f32, height: f32, margin: Margins) -> [FrameVertex; 5] {
    let left = margin.left;
    let right = (width - margin.right).max(left);
    let top = margin.top;
    let bottom = (height - margin.bottom).max(top);
    [
        (left, top),
        (right, top),
        (right, bottom),
        (left, bottom),
        (left, top),
    ]
    .map(|(x, y)| FrameVertex {
        position: px_to_ndc(x, y, width, height),
    })
}

impl RenderPipeline for FramePipeline {
    fn name(&self) -> &'static str {
        "frame"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::FullWindow
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let (w, h) = ctx.window_size;
        let verts = frame_vertices(w as f32, h as f32, ctx.inset);
        self.gpu.flush(ctx.device, ctx.queue, &verts);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}
