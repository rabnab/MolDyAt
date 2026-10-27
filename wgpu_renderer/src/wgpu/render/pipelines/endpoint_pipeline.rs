// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Endpoint marker pipeline.
//!
//! Draws a small red square with a black outline at both endpoints of the
//! highlighted wall only (never at the internal boundaries created by
//! valves), so its endpoints are easy to see and grab. See `endpoint.wgsl`.

use bytemuck::{Pod, Zeroable};
use thermo_physics::application::AppResult;
use thermo_physics::objects2d::wall_chain::ObjectRef;

use crate::wgpu::render::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::render::gpu::geometry::world_extent;
use crate::wgpu::render::gpu::shaders;
use crate::wgpu::render::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Half the marker's side, as a fraction of the world extent (tune to taste).
const MARKER_HALF_SIZE_FRAC: f32 = 0.004;
/// Outline width as a fraction of the marker's half size.
const MARKER_BORDER_FRAC: f32 = 0.4;

/// Per-marker GPU instance. Layout must match `Inst` in endpoint.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct EndpointInstance {
    pub center: [f32; 2],
    pub half_size: f32,
    pub border: f32,
}

const ENDPOINT_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32];

pub struct EndpointPipeline {
    gpu: GpuBatch<EndpointInstance>,
    /// Reused every frame to avoid reallocating
    instances: Vec<EndpointInstance>,
}

impl EndpointPipeline {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Endpoint Pipeline", shaders::ENDPOINT, &ENDPOINT_ATTRS),
            ),
            instances: Vec::new(),
        }
    }
}

impl RenderPipeline for EndpointPipeline {
    fn name(&self) -> &'static str {
        "endpoints"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::new(ctx.view_proj, [0.0; 4]));

        let half_size = world_extent(ctx.snapshot) * MARKER_HALF_SIZE_FRAC;
        let border = half_size * MARKER_BORDER_FRAC;

        self.instances.clear();
        // Only the hovered wall's own endpoints: the piece boundaries next
        // to a valve are not draggable, so they get no marker.
        let hovered_wall = match ctx.operator_previews.hovered {
            Some(ObjectRef::Wall(chain_id)) => ctx
                .snapshot
                .operators
                .walls
                .iter()
                .find(|w| w.chain_id == chain_id),
            _ => None,
        };
        if let Some(wall) = hovered_wall {
            for p in [wall.start, wall.end] {
                self.instances.push(EndpointInstance {
                    center: [p.x, p.y],
                    half_size,
                    border,
                });
            }
        }

        self.gpu.flush(ctx.device, ctx.queue, &self.instances);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}
