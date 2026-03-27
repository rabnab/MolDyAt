//! Frame border rendering pipeline for WGPU
//! Draws a thin outline around the simulation viewport in screen space.

use bytemuck::{Pod, Zeroable};
use rust_thermo_core::graphics::renderer::viewport::Margins;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameVertex {
    position: [f32; 2], // NDC position
}

pub struct FramePipeline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
}

impl FramePipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, shader_source: &str) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Frame Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<FrameVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        };

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Frame Pipeline Layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Frame Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_buffer_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineStrip,
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

        // Initialize with a dummy square; will be updated per-frame
        let initial_vertices = [FrameVertex {
            position: [0.0, 0.0],
        }; 5];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Frame Vertex Buffer"),
            contents: bytemuck::cast_slice(&initial_vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            pipeline,
            vertex_buffer,
            vertex_count: initial_vertices.len() as u32,
        }
    }

    /// Update the rectangle vertices to match the inset viewport.
    ///
    /// `margin` is in pixels; `width`/`height` are the window dimensions in pixels.
    pub fn update_vertices(
        &mut self,
        queue: &wgpu::Queue,
        width: f32,
        height: f32,
        margin: Margins,
    ) {
        // Convert screen-space rect to NDC
        let left_px = margin.left;
        let right_px = (width - margin.right).max(left_px);
        let top_px = margin.top;
        let bottom_px = (height - margin.bottom).max(top_px);

        let to_ndc = |x_px: f32, y_px: f32| -> [f32; 2] {
            let x_ndc = (x_px / width) * 2.0 - 1.0;
            // Note: screen Y goes down; NDC Y goes up
            let y_ndc = 1.0 - (y_px / height) * 2.0;
            [x_ndc, y_ndc]
        };

        let verts = [
            FrameVertex {
                position: to_ndc(left_px, top_px),
            },
            FrameVertex {
                position: to_ndc(right_px, top_px),
            },
            FrameVertex {
                position: to_ndc(right_px, bottom_px),
            },
            FrameVertex {
                position: to_ndc(left_px, bottom_px),
            },
            FrameVertex {
                position: to_ndc(left_px, top_px),
            }, // close the loop
        ];

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
    }

    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}

// ── RenderPipeline trait implementation ─────────────────────────────

use super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use rust_thermo_core::application::AppResult;

impl RenderPipeline for FramePipeline {
    fn name(&self) -> &'static str {
        "frame"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::FullWindow
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let (w, h) = ctx.window_size;
        self.update_vertices(ctx.queue, w as f32, h as f32, ctx.margin);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.draw(render_pass);
    }
}
