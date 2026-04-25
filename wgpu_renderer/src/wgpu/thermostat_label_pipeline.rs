//! Thermostat label overlay pipeline for WGPU.
//!
//! Renders local and target temperature labels near each thermostat in the
//! inset simulation viewport using the shared text shader.

use bytemuck::{Pod, Zeroable};
use font8x8::UnicodeFonts;

use super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::objects2d::SimThermostat;

const ATLAS_CHARS: [char; 15] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', '-', ':', 'L', 'T',
];
const N_ATLAS_CHARS: u32 = 15;
const GLYPH_W: u32 = 8;
const GLYPH_H: u32 = 8;

fn char_atlas_idx(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        '.' => Some(10),
        '-' => Some(11),
        ':' => Some(12),
        'L' => Some(13),
        'T' => Some(14),
        _ => None,
    }
}

fn build_atlas_data() -> Vec<u8> {
    let width = (N_ATLAS_CHARS * GLYPH_W) as usize;
    let height = GLYPH_H as usize;
    let mut data = vec![0u8; width * height];

    for (idx, &ch) in ATLAS_CHARS.iter().enumerate() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
            let x_off = idx * GLYPH_W as usize;
            for (row, &bits) in glyph.iter().enumerate() {
                for col in 0..8usize {
                    if bits & (1 << col) != 0 {
                        data[row * width + x_off + col] = 255;
                    }
                }
            }
        }
    }

    data
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TextVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

pub struct ThermostatLabelPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    atlas: Option<(wgpu::TextureView, wgpu::BindGroup)>,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: u32,
}

impl ThermostatLabelPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Thermostat Label Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text.wgsl").into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Thermostat Label Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Thermostat Label BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Thermostat Label Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Thermostat Label Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TextVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let vertex_capacity = 256;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Thermostat Label Vertex Buffer"),
            size: (vertex_capacity * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            atlas: None,
            vertex_buffer,
            vertex_capacity,
            vertex_count: 0,
        }
    }

    fn ensure_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.atlas.is_some() {
            return;
        }

        let atlas_w = N_ATLAS_CHARS * GLYPH_W;
        let atlas_h = GLYPH_H;
        let data = build_atlas_data();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Thermostat Label Atlas"),
            size: wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_w),
                rows_per_image: Some(atlas_h),
            },
            wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Thermostat Label Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.atlas = Some((view, bind_group));
    }

    fn ensure_vertex_buffer_capacity(&mut self, device: &wgpu::Device, needed: usize) {
        if needed <= self.vertex_capacity {
            return;
        }

        let cap = needed.next_power_of_two();
        self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Thermostat Label Vertex Buffer"),
            size: (cap * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.vertex_capacity = cap;
    }

    fn apply_view_proj(m: [[f32; 4]; 4], wx: f32, wy: f32) -> (f32, f32) {
        let x = m[0][0] * wx + m[1][0] * wy + m[3][0];
        let y = m[0][1] * wx + m[1][1] * wy + m[3][1];
        let w = m[0][3] * wx + m[1][3] * wy + m[3][3];
        (x / w, y / w)
    }

    pub(crate) fn build_label_text(prefix: char, value: f32) -> String {
        if value.is_finite() {
            format!("{prefix}:{value:.2}")
        } else {
            format!("{prefix}:--")
        }
    }

    fn push_label(
        verts: &mut Vec<TextVertex>,
        label: &str,
        center_x: f32,
        center_y: f32,
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) {
        let half_text_w = label.len() as f32 * glyph_ndc_w * 0.5;
        let top = center_y + glyph_ndc_h * 0.5;
        let bottom = center_y - glyph_ndc_h * 0.5;

        for (i, ch) in label.chars().enumerate() {
            let Some(atlas_idx) = char_atlas_idx(ch) else {
                continue;
            };

            let u0 = atlas_idx as f32 / N_ATLAS_CHARS as f32;
            let u1 = (atlas_idx + 1) as f32 / N_ATLAS_CHARS as f32;
            let left = center_x - half_text_w + i as f32 * glyph_ndc_w;
            let right = left + glyph_ndc_w;

            verts.push(TextVertex {
                position: [left, top],
                uv: [u0, 0.0],
            });
            verts.push(TextVertex {
                position: [left, bottom],
                uv: [u0, 1.0],
            });
            verts.push(TextVertex {
                position: [right, top],
                uv: [u1, 0.0],
            });
            verts.push(TextVertex {
                position: [left, bottom],
                uv: [u0, 1.0],
            });
            verts.push(TextVertex {
                position: [right, bottom],
                uv: [u1, 1.0],
            });
            verts.push(TextVertex {
                position: [right, top],
                uv: [u1, 0.0],
            });
        }
    }

    fn build_vertices(
        thermostats: &[SimThermostat],
        view_proj: [[f32; 4]; 4],
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) -> Vec<TextVertex> {
        let mut verts = Vec::new();

        for thermostat in thermostats {
            let local_label = Self::build_label_text('L', thermostat.current_local_temp);
            let target_label = Self::build_label_text('T', thermostat.target_temperature);

            let (top_x, top_y) = Self::apply_view_proj(
                view_proj,
                thermostat.position.x,
                thermostat.position.y + thermostat.radius,
            );
            let (bottom_x, bottom_y) = Self::apply_view_proj(
                view_proj,
                thermostat.position.x,
                thermostat.position.y - thermostat.radius,
            );

            Self::push_label(
                &mut verts,
                &local_label,
                top_x,
                top_y + glyph_ndc_h,
                glyph_ndc_w,
                glyph_ndc_h,
            );
            Self::push_label(
                &mut verts,
                &target_label,
                bottom_x,
                bottom_y - glyph_ndc_h,
                glyph_ndc_w,
                glyph_ndc_h,
            );
        }

        verts
    }

    pub(crate) fn vertex_count_for_thermostats(
        thermostats: &[SimThermostat],
        view_proj: [[f32; 4]; 4],
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) -> usize {
        Self::build_vertices(thermostats, view_proj, glyph_ndc_w, glyph_ndc_h).len()
    }
}

impl RenderPipeline for ThermostatLabelPipeline {
    fn name(&self) -> &'static str {
        "thermostat_labels"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        if ctx.snapshot.operators.thermostats.is_empty() {
            self.vertex_count = 0;
            return Ok(());
        }

        self.ensure_atlas(ctx.device, ctx.queue);

        let inset_w = (ctx.window_size.0 as f32 - ctx.margin.horizontal()).max(1.0);
        let inset_h = (ctx.window_size.1 as f32 - ctx.margin.vertical()).max(1.0);
        let glyph_ndc_w = GLYPH_W as f32 * 2.0 / inset_w;
        let glyph_ndc_h = GLYPH_H as f32 * 2.0 / inset_h;

        let verts = Self::build_vertices(
            &ctx.snapshot.operators.thermostats,
            ctx.view_proj,
            glyph_ndc_w,
            glyph_ndc_h,
        );

        if verts.is_empty() {
            self.vertex_count = 0;
            return Ok(());
        }

        self.ensure_vertex_buffer_capacity(ctx.device, verts.len());
        ctx.queue
            .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        self.vertex_count = verts.len() as u32;

        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        if self.vertex_count == 0 {
            return;
        }

        let Some((_, bind_group)) = &self.atlas else {
            return;
        };

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}
