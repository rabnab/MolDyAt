//! Line rendering pipeline for WGPU
//!
//! Renders colored lines for neighbor visualization.
//! Uses PrimitiveTopology::LineList with per-vertex colors.

use crate::application::renderer_snapshot::BallRenderData;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// GPU-side line vertex with position and color.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct LineVertex {
    /// Position in world coordinates (x, y)
    pub position: [f32; 2],
    /// RGBA color (0.0-1.0)
    pub color: [f32; 4],
}

/// Uniform buffer data for line rendering.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct LineUniforms {
    /// 4x4 view-projection matrix (world -> NDC)
    pub view_proj: [[f32; 4]; 4],
}

impl Default for LineUniforms {
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

/// Default maximum number of line vertices (each line = 2 vertices).
/// This supports up to MAX_VERTICES/2 lines.
const DEFAULT_MAX_VERTICES: usize = 65536;

/// CYAN color for main neighbor lines (matching ggez Color::CYAN)
const CYAN: [f32; 4] = [0.0, 1.0, 1.0, 1.0];

/// Gray color for secondary neighbor lines (128, 128, 128)
const GRAY: [f32; 4] = [0.502, 0.502, 0.502, 1.0];

/// Dark gray color for lines with no force (64, 64, 64)
const DARK_GRAY: [f32; 4] = [0.251, 0.251, 0.251, 1.0];

/// Line rendering pipeline and resources.
pub struct LinePipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Maximum number of vertices this buffer can hold
    max_vertices: usize,
    /// Current number of vertices to draw
    vertex_count: u32,
}

impl LinePipeline {
    /// Create a new line rendering pipeline.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, shader_source: &str) -> Self {
        Self::with_capacity(device, format, shader_source, DEFAULT_MAX_VERTICES)
    }

    /// Create a new line rendering pipeline with specified capacity.
    pub fn with_capacity(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        shader_source: &str,
        max_vertices: usize,
    ) -> Self {
        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Line Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        // Create uniform buffer
        let uniforms = LineUniforms::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Line Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create vertex buffer for line data
        let vertex_buffer_size = (max_vertices * std::mem::size_of::<LineVertex>()) as u64;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Line Vertex Buffer"),
            size: vertex_buffer_size.max(16), // Minimum 16 bytes for WGPU
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Line Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Line Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Vertex buffer layout for LineVertex
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position: vec2<f32>
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color: vec4<f32>
                wgpu::VertexAttribute {
                    offset: 8, // offset after position (2 * f32)
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Line Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Line Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_buffer_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            vertex_buffer,
            bind_group,
            max_vertices,
            vertex_count: 0,
        }
    }

    /// Update the view-projection matrix uniform.
    pub fn update_uniforms(&self, queue: &wgpu::Queue, view_proj: [[f32; 4]; 4]) {
        let uniforms = LineUniforms { view_proj };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Upload neighbor line data from ball snapshot.
    ///
    /// Generates line vertices for all balls that have show_neighbor_lines enabled.
    pub fn upload_neighbor_lines(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        balls: &[BallRenderData],
    ) {
        // Collect all line vertices
        let mut vertices = Vec::new();

        for ball in balls {
            if !ball.show_neighbor_lines {
                continue;
            }

            let start_pos = [ball.position.x, ball.position.y];

            // Find max force to determine main neighbors
            let max_force = ball
                .neighbors
                .iter()
                .map(|(_, force)| *force)
                .reduce(f32::max)
                .unwrap_or(0.0);

            for (neighbor_idx, force_magnitude) in &ball.neighbors {
                let is_main = *force_magnitude > 0.5 * max_force;

                // Skip non-main neighbors if this ball only shows main neighbors
                if ball.show_main_neighbors_only && !is_main {
                    continue;
                }

                // Determine line color based on force magnitude (matching ggez)
                let color = if *force_magnitude > 0.0 {
                    if is_main { CYAN } else { GRAY }
                } else {
                    DARK_GRAY
                };

                // Get neighbor position and create line vertices
                if let Some(neighbor) = balls.get(*neighbor_idx) {
                    let end_pos = [neighbor.position.x, neighbor.position.y];

                    // Add two vertices for the line
                    vertices.push(LineVertex {
                        position: start_pos,
                        color,
                    });
                    vertices.push(LineVertex {
                        position: end_pos,
                        color,
                    });
                }
            }
        }

        // Update vertex count
        self.vertex_count = vertices.len() as u32;

        if vertices.is_empty() {
            return;
        }

        // Resize buffer if needed
        if vertices.len() > self.max_vertices {
            self.max_vertices = vertices.len() * 2; // Double capacity
            let vertex_buffer_size = (self.max_vertices * std::mem::size_of::<LineVertex>()) as u64;
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Line Vertex Buffer"),
                size: vertex_buffer_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Upload vertex data
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    /// Render the lines.
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.vertex_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}
