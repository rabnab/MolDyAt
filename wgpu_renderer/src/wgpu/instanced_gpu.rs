//! Generic instanced-quad GPU plumbing.
//!
//! One instance = one object. The vertex shader builds a 4-corner quad from
//! `@builtin(vertex_index)` (TriangleStrip), so there is no per-vertex buffer,
//! only an instance buffer of `T`. Each object type supplies its own WGSL
//! shader and its own instance struct.
//!
//! Optionally a pipeline can also sample one texture (`new_textured`): the
//! texture and sampler are bound at `@group(1) @binding(0)` and `@binding(1)`.

use bytemuck::{Pod, Zeroable};
use std::marker::PhantomData;
use wgpu::util::DeviceExt;

/// Uniforms shared by all instanced pipelines (must match `common.wgsl`).
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct SceneUniforms {
    /// 4x4 view-projection matrix (world -> NDC)
    pub view_proj: [[f32; 4]; 4],
    /// x = time in seconds; y, z, w reserved
    pub params: [f32; 4],
}

impl Default for SceneUniforms {
    fn default() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            params: [0.0; 4],
        }
    }
}

/// Default number of instances the buffer holds before it grows.
pub const DEFAULT_CAPACITY: usize = 4096;

/// wgpu needs non-zero buffer sizes; keep a small floor.
const MIN_BUFFER_SIZE: u64 = 16;

pub struct InstancedGpu<T: Pod> {
    label: &'static str,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Optional texture + sampler at group 1
    texture_bind_group: Option<wgpu::BindGroup>,
    /// Maximum number of instances the buffer can hold
    capacity: usize,
    /// Instances to draw this frame
    count: u32,
    _marker: PhantomData<T>,
}

impl<T: Pod> InstancedGpu<T> {
    /// `attributes` describes the layout of `T` (use `wgpu::vertex_attr_array!`).
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        label: &'static str,
        shader_source: &str,
        attributes: &[wgpu::VertexAttribute],
    ) -> Self {
        Self::build(
            device,
            format,
            label,
            shader_source,
            attributes,
            DEFAULT_CAPACITY,
            None,
        )
    }

    pub fn with_capacity(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        label: &'static str,
        shader_source: &str,
        attributes: &[wgpu::VertexAttribute],
        capacity: usize,
    ) -> Self {
        Self::build(
            device,
            format,
            label,
            shader_source,
            attributes,
            capacity,
            None,
        )
    }

    /// Like `new`, but the shader can also sample `texture` (filtered, clamped)
    /// via `@group(1) @binding(0) texture_2d<f32>` and `@binding(1) sampler`.
    pub fn new_textured(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        label: &'static str,
        shader_source: &str,
        attributes: &[wgpu::VertexAttribute],
        texture: &wgpu::TextureView,
    ) -> Self {
        Self::build(
            device,
            format,
            label,
            shader_source,
            attributes,
            DEFAULT_CAPACITY,
            Some(texture),
        )
    }

    fn build(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        label: &'static str,
        shader_source: &str,
        attributes: &[wgpu::VertexAttribute],
        capacity: usize,
        texture: Option<&wgpu::TextureView>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::bytes_of(&SceneUniforms::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let instance_buffer = Self::create_instance_buffer(device, label, capacity);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Optional texture + sampler (group 1)
        let texture_layout = texture.map(|_| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
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
            })
        });

        let texture_bind_group = match (texture, texture_layout.as_ref()) {
            (Some(view), Some(layout)) => {
                let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some(label),
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                });
                Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(label),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                }))
            }
            _ => None,
        };

        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<T>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes,
        };

        let mut layouts: Vec<&wgpu::BindGroupLayout> = vec![&bind_group_layout];
        if let Some(layout) = texture_layout.as_ref() {
            layouts.push(layout);
        }

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &layouts,
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[instance_layout],
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

        Self {
            label,
            pipeline,
            uniform_buffer,
            instance_buffer,
            bind_group,
            texture_bind_group,
            capacity,
            count: 0,
            _marker: PhantomData,
        }
    }

    fn create_instance_buffer(
        device: &wgpu::Device,
        label: &'static str,
        capacity: usize,
    ) -> wgpu::Buffer {
        let size = (capacity * std::mem::size_of::<T>()) as u64;
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: size.max(MIN_BUFFER_SIZE),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Update the view-projection matrix and time uniforms.
    pub fn update_uniforms(&self, queue: &wgpu::Queue, view_proj: [[f32; 4]; 4], time: f32) {
        let uniforms = SceneUniforms {
            view_proj,
            params: [time, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Upload this frame's instances (grows the buffer if needed).
    pub fn flush(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, instances: &[T]) {
        self.count = instances.len() as u32;
        if instances.is_empty() {
            return;
        }

        if instances.len() > self.capacity {
            self.capacity = instances.len() * 2; // Double capacity
            self.instance_buffer = Self::create_instance_buffer(device, self.label, self.capacity);
        }

        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
    }

    /// Draw one quad (4 vertices) per instance.
    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        if let Some(texture_bind_group) = &self.texture_bind_group {
            render_pass.set_bind_group(1, texture_bind_group, &[]);
        }
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..4, 0..self.count);
    }
}
