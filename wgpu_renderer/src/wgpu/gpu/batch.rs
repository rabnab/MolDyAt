//! Generic GPU batch: one render pipeline, one shared uniform block, one
//! growable vertex buffer and an optional texture.
//!
//! Every pipeline in `pipelines/` draws through one or more `GpuBatch`es, so
//! bind-group layout, blending and buffer growth are defined only here.
//!
//! Two draw modes:
//! - [`DrawMode::Quads`]: one element of `T` = one instance = one quad. The
//!   vertex shader builds the 4 corners from `@builtin(vertex_index)`
//!   (TriangleStrip, see `quad_corner()` in `common.wgsl`).
//! - [`DrawMode::Vertices`]: one element of `T` = one vertex, drawn with the
//!   given topology (lines, CPU-built triangles).
//!
//! Bindings (see `common.wgsl`):
//! - `@group(0) @binding(0)`: [`SceneUniforms`] (`u`), vertex + fragment.
//! - `@group(1) @binding(0/1)`: optional texture + sampler (fragment), bound
//!   when the batch was built with [`BatchDesc::texture`].

use bytemuck::{Pod, Zeroable};
use std::marker::PhantomData;
use wgpu::util::DeviceExt;

use super::geometry::IDENTITY;

/// Uniforms shared by all pipelines (must match `Uniforms` in `common.wgsl`).
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct SceneUniforms {
    /// 4x4 view-projection matrix (world -> NDC). Identity for pipelines
    /// whose vertices are already in NDC.
    pub view_proj: [[f32; 4]; 4],
    /// Pipeline-specific values; each shader documents its own use.
    pub params: [f32; 4],
}

impl SceneUniforms {
    pub fn new(view_proj: [[f32; 4]; 4], params: [f32; 4]) -> Self {
        Self { view_proj, params }
    }

    /// Identity transform: vertices are given in NDC.
    pub fn ndc(params: [f32; 4]) -> Self {
        Self::new(IDENTITY, params)
    }
}

impl Default for SceneUniforms {
    fn default() -> Self {
        Self::ndc([0.0; 4])
    }
}

/// How the elements uploaded to a batch are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawMode {
    /// One element = one instanced quad (4-vertex TriangleStrip).
    Quads,
    /// One element = one vertex, drawn with this topology.
    Vertices(wgpu::PrimitiveTopology),
}

/// Construction parameters for a [`GpuBatch`].
pub struct BatchDesc<'a> {
    pub label: &'static str,
    /// Full WGSL source (use [`super::shaders`], which prepends `common.wgsl`).
    pub shader: &'a str,
    /// Layout of `T` (use `wgpu::vertex_attr_array!`).
    pub attributes: &'a [wgpu::VertexAttribute],
    pub mode: DrawMode,
    /// Optional texture at `@group(1)` and the filter used to sample it.
    pub texture: Option<(&'a wgpu::TextureView, wgpu::FilterMode)>,
}

impl<'a> BatchDesc<'a> {
    pub fn quads(
        label: &'static str,
        shader: &'a str,
        attributes: &'a [wgpu::VertexAttribute],
    ) -> Self {
        Self {
            label,
            shader,
            attributes,
            mode: DrawMode::Quads,
            texture: None,
        }
    }

    pub fn vertices(
        label: &'static str,
        shader: &'a str,
        attributes: &'a [wgpu::VertexAttribute],
        topology: wgpu::PrimitiveTopology,
    ) -> Self {
        Self {
            mode: DrawMode::Vertices(topology),
            ..Self::quads(label, shader, attributes)
        }
    }

    /// Sample `view` with `filter` (clamped) at `@group(1)`.
    pub fn texture(mut self, view: &'a wgpu::TextureView, filter: wgpu::FilterMode) -> Self {
        self.texture = Some((view, filter));
        self
    }
}

/// Default number of elements the buffer holds before it grows.
const DEFAULT_CAPACITY: usize = 256;

/// wgpu needs non-zero buffer sizes; keep a small floor.
const MIN_BUFFER_SIZE: u64 = 16;

/// Texture bind group plus what is needed to rebuild it for a new view.
struct TextureBinding {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
}

pub struct GpuBatch<T: Pod> {
    label: &'static str,
    mode: DrawMode,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture: Option<TextureBinding>,
    buffer: wgpu::Buffer,
    /// Maximum number of elements `buffer` can hold
    capacity: usize,
    /// Elements to draw this frame
    count: u32,
    _marker: PhantomData<T>,
}

impl<T: Pod> GpuBatch<T> {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, desc: BatchDesc) -> Self {
        let label = desc.label;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(desc.shader.into()),
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::bytes_of(&SceneUniforms::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let texture = desc
            .texture
            .map(|(view, filter)| TextureBinding::new(device, label, view, filter));

        let mut layouts = vec![&uniform_layout];
        if let Some(texture) = &texture {
            layouts.push(&texture.layout);
        }
        let stride = std::mem::size_of::<T>() as u64;
        let pipeline = create_pipeline(device, format, &desc, stride, &shader, &layouts);

        Self {
            label,
            mode: desc.mode,
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture,
            buffer: create_buffer::<T>(device, label, DEFAULT_CAPACITY),
            capacity: DEFAULT_CAPACITY,
            count: 0,
            _marker: PhantomData,
        }
    }

    /// Upload the shared uniform block.
    pub fn set_uniforms(&self, queue: &wgpu::Queue, uniforms: SceneUniforms) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Bind a new texture view (e.g. after the texture was resized).
    ///
    /// # Panics
    /// If the batch was built without a texture.
    pub fn set_texture(&mut self, device: &wgpu::Device, view: &wgpu::TextureView) {
        let texture = self
            .texture
            .as_mut()
            .expect("set_texture on a GpuBatch built without a texture");
        texture.bind_group = texture.bind(device, self.label, view);
    }

    /// Upload this frame's elements (grows the buffer if needed).
    pub fn flush(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, elements: &[T]) {
        self.count = elements.len() as u32;
        if elements.is_empty() {
            return;
        }
        if elements.len() > self.capacity {
            self.capacity = elements.len().next_power_of_two();
            self.buffer = create_buffer::<T>(device, self.label, self.capacity);
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(elements));
    }

    /// Draw nothing until the next non-empty [`flush`](Self::flush).
    pub fn clear(&mut self) {
        self.count = 0;
    }

    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        if let Some(texture) = &self.texture {
            render_pass.set_bind_group(1, &texture.bind_group, &[]);
        }
        render_pass.set_vertex_buffer(0, self.buffer.slice(..));
        match self.mode {
            DrawMode::Quads => render_pass.draw(0..4, 0..self.count),
            DrawMode::Vertices(_) => render_pass.draw(0..self.count, 0..1),
        }
    }
}

impl TextureBinding {
    fn new(
        device: &wgpu::Device,
        label: &'static str,
        view: &wgpu::TextureView,
        filter: wgpu::FilterMode,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(label),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: filter,
            min_filter: filter,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let bind_group = bind_texture(device, label, &layout, &sampler, view);
        Self {
            layout,
            sampler,
            bind_group,
        }
    }

    fn bind(
        &self,
        device: &wgpu::Device,
        label: &'static str,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        bind_texture(device, label, &self.layout, &self.sampler, view)
    }
}

fn bind_texture(
    device: &wgpu::Device,
    label: &'static str,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    desc: &BatchDesc,
    stride: u64,
    shader: &wgpu::ShaderModule,
    layouts: &[&wgpu::BindGroupLayout],
) -> wgpu::RenderPipeline {
    let (step_mode, topology) = match desc.mode {
        DrawMode::Quads => (
            wgpu::VertexStepMode::Instance,
            wgpu::PrimitiveTopology::TriangleStrip,
        ),
        DrawMode::Vertices(topology) => (wgpu::VertexStepMode::Vertex, topology),
    };

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(desc.label),
        bind_group_layouts: layouts,
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(desc.label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: stride,
                step_mode,
                attributes: desc.attributes,
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_buffer<T>(device: &wgpu::Device, label: &'static str, capacity: usize) -> wgpu::Buffer {
    let size = (capacity * std::mem::size_of::<T>()) as u64;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(MIN_BUFFER_SIZE),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
