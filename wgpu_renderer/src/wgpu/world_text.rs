use bytemuck::{Pod, Zeroable};
use font8x8::UnicodeFonts;
use rust_thermo_core::graphics::renderer::margins::Margins;
use rust_thermo_core::graphics::sidebar::sidebar_ui_scale;
use wgpu::util::DeviceExt;

pub(crate) const GLYPH_W: u32 = 8;
pub(crate) const GLYPH_H: u32 = 8;
pub(crate) const BLACK_TEXT_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TextColorUniform {
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct TextVertex {
    pub(crate) position: [f32; 2],
    pub(crate) uv: [f32; 2],
}

pub(crate) fn apply_view_proj(m: [[f32; 4]; 4], wx: f32, wy: f32) -> (f32, f32) {
    let x = m[0][0] * wx + m[1][0] * wy + m[3][0];
    let y = m[0][1] * wx + m[1][1] * wy + m[3][1];
    let w = m[0][3] * wx + m[1][3] * wy + m[3][3];
    (x / w, y / w)
}

pub(crate) fn inset_glyph_ndc_size(window_size: (u32, u32), margin: Margins) -> (f32, f32) {
    let inset_w = (window_size.0 as f32 - margin.horizontal()).max(1.0);
    let inset_h = (window_size.1 as f32 - margin.vertical()).max(1.0);
    let ui_scale = sidebar_ui_scale(margin.left);
    (
        GLYPH_W as f32 * ui_scale * 2.0 / inset_w,
        GLYPH_H as f32 * ui_scale * 2.0 / inset_h,
    )
}

pub(crate) fn push_label_vertices(
    verts: &mut Vec<TextVertex>,
    atlas_chars: &[char],
    label: &str,
    center_x: f32,
    center_y: f32,
    glyph_ndc_w: f32,
    glyph_ndc_h: f32,
) {
    let half_text_w = label.len() as f32 * glyph_ndc_w * 0.5;
    let top = center_y + glyph_ndc_h * 0.5;
    let bottom = center_y - glyph_ndc_h * 0.5;
    let atlas_len = atlas_chars.len() as f32;

    for (i, ch) in label.chars().enumerate() {
        let Some(atlas_idx) = char_atlas_idx(atlas_chars, ch) else {
            continue;
        };

        let u0 = atlas_idx as f32 / atlas_len;
        let u1 = (atlas_idx + 1) as f32 / atlas_len;
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

pub(crate) struct WorldTextPipeline {
    debug_name: &'static str,
    atlas_chars: &'static [char],
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    color_buffer: wgpu::Buffer,
    atlas: Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: u32,
}

impl WorldTextPipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        debug_name: &'static str,
        atlas_chars: &'static [char],
        initial_capacity: usize,
        color: [f32; 4],
    ) -> Self {
        let shader_label = format!("{debug_name} Shader");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&shader_label),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text.wgsl").into()),
        });

        let sampler_label = format!("{debug_name} Sampler");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&sampler_label),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout_label = format!("{debug_name} BGL");
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&bind_group_layout_label),
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

        let pipeline_layout_label = format!("{debug_name} Pipeline Layout");
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&pipeline_layout_label),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline_label = format!("{debug_name} Pipeline");
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&pipeline_label),
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
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let vertex_buffer = Self::create_vertex_buffer(device, debug_name, initial_capacity);
        let color_label = format!("{debug_name} Color Buffer");
        let color_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&color_label),
            contents: bytemuck::bytes_of(&TextColorUniform { color }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            debug_name,
            atlas_chars,
            pipeline,
            bind_group_layout,
            sampler,
            color_buffer,
            atlas: None,
            vertex_buffer,
            vertex_capacity: initial_capacity,
            vertex_count: 0,
        }
    }

    fn create_vertex_buffer(
        device: &wgpu::Device,
        debug_name: &'static str,
        capacity: usize,
    ) -> wgpu::Buffer {
        let label = format!("{debug_name} Vertex Buffer");
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&label),
            size: (capacity * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn ensure_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.atlas.is_some() {
            return;
        }

        let atlas_w = self.atlas_chars.len() as u32 * GLYPH_W;
        let atlas_h = GLYPH_H;
        let data = build_atlas_data(self.atlas_chars);

        let atlas_label = format!("{debug_name} Atlas", debug_name = self.debug_name);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&atlas_label),
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
        let bind_group_label = format!("{debug_name} Bind Group", debug_name = self.debug_name);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&bind_group_label),
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.color_buffer.as_entire_binding(),
                },
            ],
        });

        self.atlas = Some((texture, view, bind_group));
    }

    fn ensure_vertex_buffer_capacity(&mut self, device: &wgpu::Device, needed: usize) {
        if needed <= self.vertex_capacity {
            return;
        }

        self.vertex_capacity = self.vertex_capacity.max(1);
        while self.vertex_capacity < needed {
            self.vertex_capacity *= 2;
        }
        self.vertex_buffer =
            Self::create_vertex_buffer(device, self.debug_name, self.vertex_capacity);
    }

    pub(crate) fn clear(&mut self) {
        self.vertex_count = 0;
    }

    pub(crate) fn update_vertices(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        verts: &[TextVertex],
    ) {
        if verts.is_empty() {
            self.clear();
            return;
        }

        self.ensure_atlas(device, queue);
        self.ensure_vertex_buffer_capacity(device, verts.len());
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(verts));
        self.vertex_count = verts.len() as u32;
    }

    pub(crate) fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        if self.vertex_count == 0 {
            return;
        }

        let Some((_, _, bind_group)) = &self.atlas else {
            return;
        };

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}

fn char_atlas_idx(atlas_chars: &[char], c: char) -> Option<usize> {
    atlas_chars.iter().position(|&glyph| glyph == c)
}

fn build_atlas_data(atlas_chars: &[char]) -> Vec<u8> {
    let width = atlas_chars.len() * GLYPH_W as usize;
    let height = GLYPH_H as usize;
    let mut data = vec![0u8; width * height];

    for (idx, &ch) in atlas_chars.iter().enumerate() {
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
