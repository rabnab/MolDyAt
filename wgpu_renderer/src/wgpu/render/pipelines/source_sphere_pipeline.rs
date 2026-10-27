// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Source sphere rendering pipeline.
//!
//! Draws each placed source sphere as an icon quad in world space. The icon
//! depends on the sphere's state:
//!
//! - [`OperatorState::On`]  → `sourceOpen.png`  (emitting)
//! - [`OperatorState::Off`] → `sourceClosed.png` (dormant)
//!
//! Both icons share a two-slot atlas, so all spheres are one draw call.
//! See `source_sphere.wgsl`.

use bytemuck::{Pod, Zeroable};
use thermo_physics::application::AppResult;
use thermo_physics::objects2d::{OperatorState, SOURCE_SPHERE_RADIUS};

use crate::wgpu::render::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::render::gpu::shaders;
use crate::wgpu::render::gpu::textures::{ICON_SOURCE_CLOSED, ICON_SOURCE_OPEN, icon_atlas};
use crate::wgpu::render::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Atlas resolution per icon (square, same as the sidebar).
const ATLAS_CELL: u32 = 64;
const SLOT_OPEN: f32 = 0.0;
const SLOT_CLOSED: f32 = 1.0;

/// Per-sphere GPU instance. Layout must match `Inst` in source_sphere.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct SphereInstance {
    pub center: [f32; 2],
    pub half_size: f32,
    /// Atlas slot: 0 = open, 1 = closed
    pub slot: f32,
}

const SPHERE_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32];

pub struct SourceSpherePipeline {
    gpu: GpuBatch<SphereInstance>,
    /// Reused every frame to avoid reallocating
    instances: Vec<SphereInstance>,
}

impl SourceSpherePipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        // Slot order must match SLOT_OPEN / SLOT_CLOSED.
        let atlas = icon_atlas(
            device,
            queue,
            "Source Sphere Icon Atlas",
            &[ICON_SOURCE_OPEN, ICON_SOURCE_CLOSED],
            ATLAS_CELL,
        );
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Source Sphere", shaders::SOURCE_SPHERE, &SPHERE_ATTRS)
                    .texture(&atlas, wgpu::FilterMode::Linear),
            ),
            instances: Vec::new(),
        }
    }
}

impl RenderPipeline for SourceSpherePipeline {
    fn name(&self) -> &'static str {
        "source_spheres"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::new(ctx.view_proj, [0.0; 4]));

        self.instances.clear();
        self.instances
            .extend(
                ctx.snapshot
                    .operators
                    .source_spheres
                    .iter()
                    .map(|sphere| SphereInstance {
                        center: [sphere.center.x, sphere.center.y],
                        half_size: SOURCE_SPHERE_RADIUS,
                        slot: if sphere.state == OperatorState::On {
                            SLOT_OPEN
                        } else {
                            SLOT_CLOSED
                        },
                    }),
            );
        self.gpu.flush(ctx.device, ctx.queue, &self.instances);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}
