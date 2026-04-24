//! Source sphere rendering pipeline.
//!
//! Renders each placed [`SimSourceSphere`] as a textured quad in world
//! space.  The icon shown depends on the sphere's state:
//!
//! - [`OperatorState::On`]  → `sourceOpen.png`  (emitting, open)
//! - [`OperatorState::Off`] → `sourceClosed.png` (dormant, closed)
//!
//! Both icons are packed into a two-slot vertical atlas so a single
//! draw call can render all spheres using one UV coordinate range per
//! state.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use super::sidebar_pipeline::decode_icon;
use rust_thermo_core::application::AppResult;
use rust_thermo_core::objects2d::{OperatorState, SOURCE_SPHERE_RADIUS};

/// Embedded icon assets.
const ICON_SOURCE_OPEN: &[u8] = include_bytes!("icons/sourceOpen.png");
const ICON_SOURCE_CLOSED: &[u8] = include_bytes!("icons/sourceClosed.png");

/// Atlas resolution per icon (square, same as sidebar).
const ATLAS_CELL: u32 = 64;

/// Number of atlas slots (open + closed).
const ATLAS_SLOTS: u32 = 2;

/// Maximum number of source spheres we can render per frame.
const MAX_SPHERES: usize = 256;

/// 6 vertices per quad.
const VERTS_PER_QUAD: usize = 6;

/// GPU vertex for a source sphere quad.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SphereVertex {
    /// World-space position. The shader applies `view_proj` to transform it.
    position: [f32; 2],
    /// UV coordinate in the atlas.
    uv: [f32; 2],
}

/// Uniform buffer: 4×4 view-projection matrix.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SphereUniforms {
    view_proj: [[f32; 4]; 4],
}

impl Default for SphereUniforms {
    fn default() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
}

const SPHERE_SHADER: &str = include_str!("shaders/source_sphere.wgsl");

/// Pipeline that renders placed source spheres as icons.
pub struct SourceSpherePipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    vertex_count: u32,
}

impl SourceSpherePipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        // ── Build atlas: [sourceOpen, sourceClosed] stacked vertically ──
        let open_px = decode_icon(ICON_SOURCE_OPEN, ATLAS_CELL);
        let closed_px = decode_icon(ICON_SOURCE_CLOSED, ATLAS_CELL);

        let atlas_w = ATLAS_CELL;
        let atlas_h = ATLAS_CELL * ATLAS_SLOTS;
        let mut atlas_data = Vec::with_capacity((atlas_w * atlas_h * 4) as usize);
        atlas_data.extend_from_slice(&open_px);
        atlas_data.extend_from_slice(&closed_px);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Source Sphere Icon Atlas"),
            size: wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            &atlas_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_w * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Source Sphere Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Uniform buffer ───────────────────────────────────────────
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Source Sphere Uniform Buffer"),
            contents: bytemuck::bytes_of(&SphereUniforms::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Bind group ───────────────────────────────────────────────
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Source Sphere BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Source Sphere Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // ── Shader / pipeline ────────────────────────────────────────
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Source Sphere Shader"),
            source: wgpu::ShaderSource::Wgsl(SPHERE_SHADER.into()),
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SphereVertex>() as u64,
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
            label: Some("Source Sphere Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Source Sphere Pipeline"),
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
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // ── Vertex buffer ────────────────────────────────────────────
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Source Sphere Vertex Buffer"),
            size: (MAX_SPHERES * VERTS_PER_QUAD * std::mem::size_of::<SphereVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group,
            vertex_buffer,
            uniform_buffer,
            vertex_count: 0,
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
        // Update view-projection matrix.
        ctx.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&SphereUniforms {
                view_proj: ctx.view_proj,
            }),
        );

        let spheres = &ctx.snapshot.operators.source_spheres;
        if spheres.is_empty() {
            self.vertex_count = 0;
            return Ok(());
        }

        if spheres.len() > MAX_SPHERES {
            log::warn!(
                "source sphere render limit reached: drawing first {} of {} spheres",
                MAX_SPHERES,
                spheres.len()
            );
            debug_assert!(
                spheres.len() <= MAX_SPHERES,
                "source sphere pipeline truncated {} spheres beyond MAX_SPHERES={}",
                spheres.len() - MAX_SPHERES,
                MAX_SPHERES
            );
        }

        let mut verts: Vec<SphereVertex> = Vec::with_capacity(spheres.len() * VERTS_PER_QUAD);

        for sphere in spheres.iter().take(MAX_SPHERES) {
            let cx = sphere.center.x;
            let cy = sphere.center.y;
            let r = SOURCE_SPHERE_RADIUS;

            // World-space corners of the icon quad.
            let x0 = cx - r;
            let x1 = cx + r;
            let y0 = cy - r;
            let y1 = cy + r;

            // UV row: 0.0 = sourceOpen (On), 0.5 = sourceClosed (Off).
            let uv_top = if sphere.state == OperatorState::On {
                0.0_f32
            } else {
                0.5_f32
            };
            let uv_bot = uv_top + 0.5;

            // Two triangles (CCW).
            let corners = [
                ([x0, y0], [0.0_f32, uv_top]),
                ([x0, y1], [0.0, uv_bot]),
                ([x1, y0], [1.0, uv_top]),
                ([x1, y0], [1.0, uv_top]),
                ([x0, y1], [0.0, uv_bot]),
                ([x1, y1], [1.0, uv_bot]),
            ];
            for (pos, uv) in corners {
                verts.push(SphereVertex { position: pos, uv });
            }
        }

        self.vertex_count = verts.len() as u32;
        if !verts.is_empty() {
            ctx.queue
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        }

        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        if self.vertex_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}
