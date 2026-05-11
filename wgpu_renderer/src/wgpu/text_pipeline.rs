//! Simple ASCII text overlay pipeline for WGPU.
//! Renders monospaced 8x8 bitmap glyphs in screen space using an R8 texture atlas.

use bytemuck::{Pod, Zeroable};
use font8x8::UnicodeFonts;
use rust_thermo_core::graphics::renderer::margins::Margins;
use rust_thermo_core::graphics::sidebar::sidebar_ui_scale;
use wgpu::util::DeviceExt;

const CURSOR_TEXT_COLOR: [f32; 4] = [0.92, 0.9, 0.84, 1.0];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TextColorUniform {
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TextVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

/// Screen-space text renderer for small debug overlays.
pub struct TextPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    color_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    texture_size: (u32, u32),
}

impl TextPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text.wgsl").into()),
        });

        // Minimal texture to start; resized on first update.
        let texture_size = (8, 8);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Text Texture"),
            size: wgpu::Extent3d {
                width: texture_size.0,
                height: texture_size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Text Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Text Bind Group Layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let color_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Text Color Buffer"),
            contents: bytemuck::bytes_of(&TextColorUniform {
                color: CURSOR_TEXT_COLOR,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: color_buffer.as_entire_binding(),
                },
            ],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
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
        };

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_layout],
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
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Text Vertex Buffer"),
            contents: bytemuck::cast_slice(
                &[TextVertex {
                    position: [0.0, 0.0],
                    uv: [0.0, 0.0],
                }; 4],
            ),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            pipeline,
            bind_group_layout,
            bind_group,
            texture,
            texture_view,
            sampler,
            color_buffer,
            vertex_buffer,
            vertex_count: 4,
            texture_size,
        }
    }

    fn recreate_texture(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Text Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.texture_view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        // Rebind with new view
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.color_buffer.as_entire_binding(),
                },
            ],
        });
        self.texture_size = (width, height);
    }

    /// Update glyph texture and quad placement for the provided text.
    pub fn update_text(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        window_size: (u32, u32),
        margin: Margins,
    ) {
        let width_px = (text.len().max(1) as u32) * 8;
        let height_px = 8u32;

        if width_px > self.texture_size.0 || height_px > self.texture_size.1 {
            self.recreate_texture(
                device,
                width_px.max(self.texture_size.0),
                height_px.max(self.texture_size.1),
            );
        }

        let mut buffer = vec![0u8; (width_px * height_px) as usize];
        for (i, ch) in text.chars().enumerate() {
            if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
                for (row, bits) in glyph.iter().enumerate() {
                    for col in 0..8 {
                        if bits & (1 << col) != 0 {
                            let x = (i as u32 * 8) + col;
                            let y = row as u32;
                            let idx = (y * width_px + x) as usize;
                            buffer[idx] = 255;
                        }
                    }
                }
            }
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &buffer,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width_px),
                rows_per_image: Some(height_px),
            },
            wgpu::Extent3d {
                width: width_px,
                height: height_px,
                depth_or_array_layers: 1,
            },
        );

        let [u_max, v_max] = active_text_uv_max(self.texture_size, (width_px, height_px));

        // Position: bottom-right of the simulation pane, inside the bottom margin.
        let win_w = window_size.0 as f32;
        let win_h = window_size.1 as f32;
        let ui_scale = sidebar_ui_scale(margin.left);
        let text_w = width_px as f32 * ui_scale;
        let text_h = height_px as f32 * ui_scale;
        let padding = 4.0 * ui_scale;
        let left_px = (win_w - margin.right - text_w - padding).max(margin.left);
        let top_px = (win_h - margin.bottom + padding).max(0.0);
        let right_px = left_px + text_w;
        let bottom_px = top_px + text_h;

        let to_ndc = |x: f32, y: f32| -> [f32; 2] {
            let x_ndc = (x / win_w) * 2.0 - 1.0;
            let y_ndc = 1.0 - (y / win_h) * 2.0;
            [x_ndc, y_ndc]
        };

        let verts = [
            TextVertex {
                position: to_ndc(left_px, top_px),
                uv: [0.0, 0.0],
            },
            TextVertex {
                position: to_ndc(left_px, bottom_px),
                uv: [0.0, v_max],
            },
            TextVertex {
                position: to_ndc(right_px, top_px),
                uv: [u_max, 0.0],
            },
            TextVertex {
                position: to_ndc(right_px, bottom_px),
                uv: [u_max, v_max],
            },
        ];

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
    }

    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}

pub(super) fn active_text_uv_max(texture_size: (u32, u32), content_size: (u32, u32)) -> [f32; 2] {
    let u_max = if texture_size.0 == 0 {
        1.0
    } else {
        content_size.0 as f32 / texture_size.0 as f32
    };
    let v_max = if texture_size.1 == 0 {
        1.0
    } else {
        content_size.1 as f32 / texture_size.1 as f32
    };

    [u_max.clamp(0.0, 1.0), v_max.clamp(0.0, 1.0)]
}

// ── RenderPipeline trait implementation ─────────────────────────────

use super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use rust_thermo_core::application::AppResult;

impl RenderPipeline for TextPipeline {
    fn name(&self) -> &'static str {
        "text"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::FullWindow
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let cursor_text = format!(
            "({:.2}, {:.2})  {}g",
            ctx.cursor_world_pos.0, ctx.cursor_world_pos.1, ctx.snapshot.gravity_force_multiple
        );
        self.update_text(
            ctx.device,
            ctx.queue,
            &cursor_text,
            ctx.window_size,
            ctx.margin,
        );
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.draw(render_pass);
    }
}
