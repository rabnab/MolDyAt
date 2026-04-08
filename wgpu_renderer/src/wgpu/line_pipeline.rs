//! Line rendering pipeline for WGPU
//!
//! Renders colored lines for neighbor visualization.
//! Uses PrimitiveTopology::LineList with per-vertex colors.
//!
//! Line data is collected via the shared [`BatchRenderer<LineSegment>`]
//! abstraction and flushed into a GPU vertex buffer here.

use bytemuck::{Pod, Zeroable};
use rust_thermo_core::graphics::renderer::batch_renderer::{BatchRenderer, LineSegment};
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
            immediate_size: 0,
        });

        // Create render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Line Pipeline"),
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
            multiview_mask: None,
            cache: None,
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

    /// Flush a [`BatchRenderer<LineSegment>`] into the GPU vertex buffer.
    ///
    /// Converts each [`LineSegment`] (world-coordinate line) into a pair
    /// of [`LineVertex`] structs and uploads them.  Drains the batch.
    pub fn flush_line_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: &mut BatchRenderer<LineSegment>,
    ) {
        if batch.is_empty() {
            self.vertex_count = 0;
            return;
        }

        // Convert LineSegments → LineVertex pairs
        let mut vertices: Vec<LineVertex> = Vec::with_capacity(batch.len() * 2);
        for seg in batch.drain() {
            vertices.push(LineVertex {
                position: seg.points[0],
                color: seg.color,
            });
            vertices.push(LineVertex {
                position: seg.points[1],
                color: seg.color,
            });
        }

        self.vertex_count = vertices.len() as u32;

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
    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.vertex_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}

// ── RenderPipeline trait implementation ─────────────────────────────

use super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::graphics::renderer::batch_renderer::collect_neighbor_lines;

impl RenderPipeline for LinePipeline {
    fn name(&self) -> &'static str {
        "lines"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        self.update_uniforms(ctx.queue, ctx.view_proj);
        let mut line_batch: BatchRenderer<LineSegment> = BatchRenderer::new();
        collect_neighbor_lines(&ctx.snapshot.balls, &mut line_batch);

        // Pre-compute world-extent-based sizing for indicators
        let bounds = &ctx.snapshot.cell_grid_bounds;
        let world_extent = bounds.w.abs().max(bounds.h.abs()).max(1.0);

        // Committed line segments (white, opaque)
        for seg in &ctx.snapshot.line_segments {
            line_batch.push(LineSegment {
                points: [[seg.start.x, seg.start.y], [seg.end.x, seg.end.y]],
                width: 2.0,
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }

        // Valve segment overlays — draw on top of committed line segments
        for valve in &ctx.snapshot.valve_segments {
            if let Some(seg) = ctx
                .snapshot
                .line_segments
                .iter()
                .find(|s| s.id == valve.line_id)
            {
                let p0 = seg.point_at_t(valve.t_start);
                let p1 = seg.point_at_t(valve.t_end);

                if valve.is_closed {
                    // Closed valve: thick green line
                    line_batch.push(LineSegment {
                        points: [[p0.x, p0.y], [p1.x, p1.y]],
                        width: 3.0,
                        color: [0.0, 0.9, 0.2, 1.0],
                    });
                } else {
                    // Open valve: red dashed effect (3 short segments)
                    let dx = p1.x - p0.x;
                    let dy = p1.y - p0.y;
                    let segments = 3;
                    for i in 0..segments {
                        let a = i as f32 / segments as f32;
                        let b = (i as f32 + 0.5) / segments as f32;
                        line_batch.push(LineSegment {
                            points: [
                                [p0.x + dx * a, p0.y + dy * a],
                                [p0.x + dx * b, p0.y + dy * b],
                            ],
                            width: 2.0,
                            color: [0.9, 0.2, 0.1, 0.6],
                        });
                    }
                }

                // State indicator at midpoint
                let mid_x = (p0.x + p1.x) * 0.5;
                let mid_y = (p0.y + p1.y) * 0.5;
                let indicator_size = world_extent * 0.012;

                if valve.is_closed {
                    // Green circle indicator (approximated with small crosshair)
                    let ic = [0.0, 1.0, 0.3, 1.0];
                    line_batch.push(LineSegment {
                        points: [
                            [mid_x - indicator_size, mid_y],
                            [mid_x + indicator_size, mid_y],
                        ],
                        width: 2.0,
                        color: ic,
                    });
                    line_batch.push(LineSegment {
                        points: [
                            [mid_x, mid_y - indicator_size],
                            [mid_x, mid_y + indicator_size],
                        ],
                        width: 2.0,
                        color: ic,
                    });
                } else {
                    // Red X indicator
                    let ic = [1.0, 0.2, 0.1, 0.9];
                    line_batch.push(LineSegment {
                        points: [
                            [mid_x - indicator_size, mid_y - indicator_size],
                            [mid_x + indicator_size, mid_y + indicator_size],
                        ],
                        width: 2.0,
                        color: ic,
                    });
                    line_batch.push(LineSegment {
                        points: [
                            [mid_x + indicator_size, mid_y - indicator_size],
                            [mid_x - indicator_size, mid_y + indicator_size],
                        ],
                        width: 2.0,
                        color: ic,
                    });
                }
            }
        }

        // Ghost preview for valve segment drawing (semi-transparent magenta)
        if let Some((line_id, t_start, t_end)) = ctx.valve_segment_preview
            && let Some(seg) = ctx.snapshot.line_segments.iter().find(|s| s.id == line_id)
        {
            let p0 = seg.point_at_t(t_start);
            let p1 = seg.point_at_t(t_end);
            line_batch.push(LineSegment {
                points: [[p0.x, p0.y], [p1.x, p1.y]],
                width: 3.0,
                color: [1.0, 0.0, 1.0, 0.5],
            });
        }

        // Ghost preview (semi-transparent yellow)
        if let Some((start, end)) = ctx.line_segment_preview {
            line_batch.push(LineSegment {
                points: [start, end],
                width: 2.0,
                color: [1.0, 1.0, 0.0, 0.5],
            });
        }

        // Snap indicators: small crosshairs at snap target positions.
        // Scale the crosshair size proportionally to the simulation region
        // so it stays visually consistent when the world is resized.
        let cross_size = world_extent * 0.015;
        let snap_color = [0.0, 1.0, 1.0, 0.9]; // cyan
        for &pos in &ctx.snap_indicators {
            // Horizontal bar
            line_batch.push(LineSegment {
                points: [[pos[0] - cross_size, pos[1]], [pos[0] + cross_size, pos[1]]],
                width: 2.0,
                color: snap_color,
            });
            // Vertical bar
            line_batch.push(LineSegment {
                points: [[pos[0], pos[1] - cross_size], [pos[0], pos[1] + cross_size]],
                width: 2.0,
                color: snap_color,
            });
        }

        self.flush_line_batch(ctx.device, ctx.queue, &mut line_batch);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.draw(render_pass);
    }
}
