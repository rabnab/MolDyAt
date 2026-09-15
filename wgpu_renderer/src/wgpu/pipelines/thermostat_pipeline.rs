//! Thermostat rendering pipeline.
//!
//! Each thermostat is drawn as:
//! 1. a semi-transparent zone over its range of influence (pulsing when the
//!    thermostat is on, constant when off) -- see `thermostat_zone.wgsl`;
//! 2. the `thermo.png` icon at its position -- see `thermostat_icon.wgsl`.
//!
//! Replaces the thermostat block that used to live in `LinePipeline::prepare`.

use super::super::instanced_gpu::InstancedGpu;
use super::super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use bytemuck::{Pod, Zeroable};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::objects2d::OperatorState;
use std::time::Instant;
use wgpu::util::DeviceExt;

/// Icon half-size as a fraction of the world extent (tune to taste).
const ICON_HALF_SIZE_FRAC: f32 = 0.02;

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
    zone: InstancedGpu<ZoneInstance>,
    icon: InstancedGpu<IconInstance>,
    zone_instances: Vec<ZoneInstance>,
    icon_instances: Vec<IconInstance>,
    /// Animation clock for the pulse
    start: Instant,
}

impl ThermostatPipeline {
    /// `icon_png` is the raw bytes of the PNG, e.g.
    /// `include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/thermo.png"))`.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        icon_png: &[u8],
    ) -> Self {
        let common = include_str!("../shaders/common.wgsl");
        let zone_src = format!(
            "{}\n{}",
            common,
            include_str!("../shaders/thermostat_zone.wgsl")
        );
        let icon_src = format!(
            "{}\n{}",
            common,
            include_str!("../shaders/thermostat_icon.wgsl")
        );

        let icon_view = load_icon_texture(device, queue, icon_png);

        Self {
            zone: InstancedGpu::new(device, format, "Thermostat Zone", &zone_src, &ZONE_ATTRS),
            icon: InstancedGpu::new_textured(
                device,
                format,
                "Thermostat Icon",
                &icon_src,
                &ICON_ATTRS,
                &icon_view,
            ),
            zone_instances: Vec::new(),
            icon_instances: Vec::new(),
            start: Instant::now(),
        }
    }
}

/// Decode a PNG and upload it with a full mip chain (so the icon stays clean
/// when zoomed out).
fn load_icon_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    png_bytes: &[u8],
) -> wgpu::TextureView {
    let img = image::load_from_memory(png_bytes)
        .expect("thermo.png must be a valid image")
        .to_rgba8();
    let (width, height) = img.dimensions();

    // floor(log2(max side)) + 1 levels; wgpu mip sizes are max(1, dim >> level).
    let mip_count = 32 - width.max(height).leading_zeros();
    let mut data: Vec<u8> = Vec::new();
    let mut level = img;
    for i in 0..mip_count {
        data.extend_from_slice(level.as_raw());
        if i + 1 < mip_count {
            let (w, h) = ((level.width() / 2).max(1), (level.height() / 2).max(1));
            level = image::imageops::resize(&level, w, h, image::imageops::FilterType::Triangle);
        }
    }

    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("Thermostat Icon Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &data,
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
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
        self.zone.update_uniforms(ctx.queue, ctx.view_proj, time);
        self.icon.update_uniforms(ctx.queue, ctx.view_proj, time);

        let bounds = &ctx.snapshot.cell_grid_bounds;
        let world_extent = bounds.w.abs().max(bounds.h.abs()).max(1.0);
        let icon_half_size = world_extent * ICON_HALF_SIZE_FRAC;

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
