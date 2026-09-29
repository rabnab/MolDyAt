//! Thermostat rendering pipeline.
//!
//! Each thermostat is drawn as:
//! 1. a semi-transparent zone over its range of influence (pulsing when the
//!    thermostat is on, constant when off) -- see `thermostat_zone.wgsl`;
//! 2. the `thermo.png` icon at its position -- see `thermostat_icon.wgsl`.

use bytemuck::{Pod, Zeroable};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::objects2d::OperatorState;
use std::time::Instant;

use crate::wgpu::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::gpu::geometry::world_extent;
use crate::wgpu::gpu::shaders;
use crate::wgpu::gpu::textures::{ICON_THERMO, mipmapped_icon};
use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Icon half-size as a fraction of the world extent (tune to taste).
pub(crate) const ICON_HALF_SIZE_FRAC: f32 = 0.02;

/// Zone instance. Layout must match `Inst` in thermostat_zone.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct ZoneInstance {
    pub center: [f32; 2],
    pub radius: f32,
    /// Target temperature, 0..1
    pub temp: f32,
    /// 1.0 = on, 0.0 = off
    pub is_on: f32,
}

/// Icon instance. Layout must match `Inst` in thermostat_icon.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct IconInstance {
    pub center: [f32; 2],
    pub half_size: f32,
}

const ZONE_ATTRS: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32];
const ICON_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32];

pub struct ThermostatPipeline {
    zone: GpuBatch<ZoneInstance>,
    icon: GpuBatch<IconInstance>,
    zone_instances: Vec<ZoneInstance>,
    icon_instances: Vec<IconInstance>,
    /// Animation clock for the pulse
    start: Instant,
}

impl ThermostatPipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let icon_view = mipmapped_icon(device, queue, "Thermostat Icon", ICON_THERMO);

        Self {
            zone: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Thermostat Zone", shaders::THERMOSTAT_ZONE, &ZONE_ATTRS),
            ),
            icon: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Thermostat Icon", shaders::THERMOSTAT_ICON, &ICON_ATTRS)
                    .texture(&icon_view, wgpu::FilterMode::Linear),
            ),
            zone_instances: Vec::new(),
            icon_instances: Vec::new(),
            start: Instant::now(),
        }
    }
}

impl RenderPipeline for ThermostatPipeline {
    fn name(&self) -> &'static str {
        "thermostats"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let time = self.start.elapsed().as_secs_f32();
        let uniforms = SceneUniforms::new(ctx.view_proj, [time, 0.0, 0.0, 0.0]);
        self.zone.set_uniforms(ctx.queue, uniforms);
        self.icon.set_uniforms(ctx.queue, uniforms);

        let icon_half_size = world_extent(ctx.snapshot) * ICON_HALF_SIZE_FRAC;

        self.zone_instances.clear();
        self.icon_instances.clear();

        for thermostat in &ctx.snapshot.operators.thermostats {
            let center = [thermostat.position.x, thermostat.position.y];

            if thermostat.radius > f32::EPSILON {
                self.zone_instances.push(ZoneInstance {
                    center,
                    radius: thermostat.radius,
                    temp: thermostat.target_temperature.clamp(0.0, 1.0),
                    is_on: if thermostat.state == OperatorState::On {
                        1.0
                    } else {
                        0.0
                    },
                });
            }

            self.icon_instances.push(IconInstance {
                center,
                half_size: icon_half_size,
            });
        }

        self.zone.flush(ctx.device, ctx.queue, &self.zone_instances);
        self.icon.flush(ctx.device, ctx.queue, &self.icon_instances);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        // Zones first, icons on top.
        self.zone.draw(render_pass);
        self.icon.draw(render_pass);
    }
}
