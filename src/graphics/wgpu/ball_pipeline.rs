//! Ball rendering pipeline for WGPU
//!
//! Renders balls as instanced quads with SDF circles for anti-aliased edges.
//! Uses a storage buffer to efficiently upload ball data to the GPU.
//!
//! # Design
//! - Single draw call for all balls (instanced rendering)
//! - Storage buffer holds packed ball data (position, radius, color)
//! - Fragment shader computes perfect circles using SDF with smooth edges
//! - Supports dynamic ball counts up to a configurable maximum

use crate::application::renderer_snapshot::BallRenderData;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// GPU-side ball data structure.
/// Must match the `BallData` struct in ball.wgsl exactly.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GpuBallData {
    /// Center position in world coordinates (x, y)
    pub position: [f32; 2],
    /// Ball radius in world units
    pub radius: f32,
    /// Packed RGBA color as u32 (0xAABBGGRR format, little-endian)
    pub color: u32,
}

impl GpuBallData {
    /// Create from render data, packing color into u32.
    pub fn from_render_data(data: &BallRenderData) -> Self {
        Self {
            position: [data.position.x, data.position.y],
            radius: data.radius,
            color: pack_color(data.color),
        }
    }
}

/// Pack RGBA [0.0-1.0] color into u32 (0xAABBGGRR format, little-endian).
fn pack_color(color: [f32; 4]) -> u32 {
    let r = (color[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (color[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (color[2].clamp(0.0, 1.0) * 255.0) as u32;
    let a = (color[3].clamp(0.0, 1.0) * 255.0) as u32;
    r | (g << 8) | (b << 16) | (a << 24)
}

/// Uniform buffer data for ball rendering.
/// Must match the `Uniforms` struct in ball.wgsl exactly.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct BallUniforms {
    /// 4x4 view-projection matrix (world -> NDC)
    pub view_proj: [[f32; 4]; 4],
    /// Screen size (width, height) in pixels
    pub screen_size: [f32; 2],
    /// Equilibrium edge distance (from LJ potential, typically ~0.56 for lj_sig=0.5)
    pub eq_edge_distance: f32,
    /// Cutoff distance for interactions (typically 4.0)
    pub cutoff_distance: f32,
    /// Show cutoff rings flag (0 = hide, 1 = show)
    pub show_cutoff_rings: u32,
    /// Padding to maintain 16-byte alignment
    _padding: [u32; 3],
}

impl Default for BallUniforms {
    fn default() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            screen_size: [1280.0, 720.0],
            // Default equilibrium edge distance (√(6/2) * 0.5 ≈ 0.56)
            eq_edge_distance: 0.56,
            // Default cutoff distance
            cutoff_distance: 4.0,
            // Show cutoff rings by default
            show_cutoff_rings: 1,
            _padding: [0; 3],
        }
    }
}

/// Default maximum number of balls the storage buffer can hold.
/// Can be increased at runtime by recreating the buffer.
const DEFAULT_MAX_BALLS: usize = 4096;

/// Ball rendering pipeline and resources.
///
/// Manages the GPU resources needed for efficient instanced ball rendering:
/// - Render pipeline with vertex/fragment shaders
/// - Storage buffer for ball data
/// - Uniform buffer for viewport transformation
pub struct BallPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    storage_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    uniforms: BallUniforms,
    /// Maximum number of balls this buffer can hold
    max_balls: usize,
    /// Current number of balls uploaded to GPU
    ball_count: usize,
}

impl BallPipeline {
    /// Create a new ball rendering pipeline.
    ///
    /// # Arguments
    /// * `device` - WGPU device
    /// * `format` - Surface texture format
    /// * `shader_source` - WGSL shader source code
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, shader_source: &str) -> Self {
        Self::with_capacity(device, format, shader_source, DEFAULT_MAX_BALLS)
    }

    /// Create a new ball rendering pipeline with specified capacity.
    ///
    /// # Arguments
    /// * `device` - WGPU device
    /// * `format` - Surface texture format
    /// * `shader_source` - WGSL shader source code
    /// * `max_balls` - Maximum number of balls to support
    pub fn with_capacity(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        shader_source: &str,
        max_balls: usize,
    ) -> Self {
        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Ball Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        // Create uniform buffer
        let uniforms = BallUniforms::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Ball Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create storage buffer for ball data
        // Size = max_balls * size_of::<GpuBallData>
        let storage_buffer_size = (max_balls * std::mem::size_of::<GpuBallData>()) as u64;
        let storage_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Ball Storage Buffer"),
            size: storage_buffer_size.max(16), // Minimum 16 bytes for WGPU
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Ball Bind Group Layout"),
            entries: &[
                // Uniform buffer (binding 0)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Storage buffer (binding 1)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Ball Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: storage_buffer.as_entire_binding(),
                },
            ],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Ball Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Ball Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[], // No vertex buffers - using storage buffer + instancing
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
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // No culling for 2D quads
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
            storage_buffer,
            bind_group_layout,
            bind_group,
            uniforms,
            max_balls,
            ball_count: 0,
        }
    }

    /// Update the view-projection matrix and screen size.
    ///
    /// # Arguments
    /// * `queue` - WGPU queue for buffer writes
    /// * `view_proj` - View-projection matrix (world -> NDC)
    /// * `screen_size` - Screen dimensions in pixels
    /// * `eq_edge_distance` - Equilibrium edge distance from physics
    /// * `cutoff_distance` - Cutoff distance for interactions
    pub fn update_uniforms(
        &mut self,
        queue: &wgpu::Queue,
        view_proj: [[f32; 4]; 4],
        screen_size: [f32; 2],
        eq_edge_distance: f32,
        cutoff_distance: f32,
    ) {
        self.uniforms.view_proj = view_proj;
        self.uniforms.screen_size = screen_size;
        self.uniforms.eq_edge_distance = eq_edge_distance;
        self.uniforms.cutoff_distance = cutoff_distance;
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniforms]),
        );
    }

    /// Toggle cutoff ring visibility.
    ///
    /// # Arguments
    /// * `queue` - WGPU queue for buffer writes
    pub fn toggle_cutoff_rings(&mut self, queue: &wgpu::Queue) {
        self.uniforms.show_cutoff_rings = if self.uniforms.show_cutoff_rings == 0 {
            1
        } else {
            0
        };
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniforms]),
        );
    }

    /// Set cutoff ring visibility.
    ///
    /// # Arguments
    /// * `queue` - WGPU queue for buffer writes
    /// * `visible` - Whether cutoff rings should be visible
    pub fn set_cutoff_rings_visible(&mut self, queue: &wgpu::Queue, visible: bool) {
        self.uniforms.show_cutoff_rings = if visible { 1 } else { 0 };
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniforms]),
        );
    }

    /// Check if cutoff rings are currently visible.
    pub fn are_cutoff_rings_visible(&self) -> bool {
        self.uniforms.show_cutoff_rings != 0
    }

    /// Upload ball data to the GPU storage buffer.
    ///
    /// If the number of balls exceeds max_balls, the buffer is reallocated.
    ///
    /// # Arguments
    /// * `device` - WGPU device (needed if buffer reallocation required)
    /// * `queue` - WGPU queue for buffer writes
    /// * `balls` - Slice of ball render data
    pub fn upload_balls(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        balls: &[BallRenderData],
    ) {
        self.ball_count = balls.len();

        if self.ball_count == 0 {
            return;
        }

        // Check if we need to reallocate the storage buffer
        if self.ball_count > self.max_balls {
            self.reallocate_storage_buffer(device, self.ball_count * 2);
        }

        // Convert ball data to GPU format
        let gpu_balls: Vec<GpuBallData> = balls.iter().map(GpuBallData::from_render_data).collect();

        // Upload to GPU
        queue.write_buffer(&self.storage_buffer, 0, bytemuck::cast_slice(&gpu_balls));
    }

    /// Reallocate the storage buffer with a new capacity.
    fn reallocate_storage_buffer(&mut self, device: &wgpu::Device, new_max_balls: usize) {
        self.max_balls = new_max_balls;
        let storage_buffer_size = (self.max_balls * std::mem::size_of::<GpuBallData>()) as u64;

        self.storage_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Ball Storage Buffer"),
            size: storage_buffer_size.max(16),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Recreate bind group with new storage buffer
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Ball Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.storage_buffer.as_entire_binding(),
                },
            ],
        });

        println!(
            "Ball storage buffer reallocated: {} balls capacity",
            self.max_balls
        );
    }

    /// Render all balls in a single instanced draw call.
    ///
    /// # Arguments
    /// * `render_pass` - Active render pass
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.ball_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        // 6 vertices per quad (2 triangles), one instance per ball
        render_pass.draw(0..6, 0..self.ball_count as u32);
    }

    /// Get the current number of balls uploaded to GPU.
    pub fn ball_count(&self) -> usize {
        self.ball_count
    }

    /// Get the maximum ball capacity of the current buffer.
    pub fn max_balls(&self) -> usize {
        self.max_balls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_color() {
        // White with full alpha
        let white = pack_color([1.0, 1.0, 1.0, 1.0]);
        assert_eq!(white, 0xFFFFFFFF);

        // Red with full alpha
        let red = pack_color([1.0, 0.0, 0.0, 1.0]);
        assert_eq!(red, 0xFF0000FF);

        // Green with full alpha
        let green = pack_color([0.0, 1.0, 0.0, 1.0]);
        assert_eq!(green, 0xFF00FF00);

        // Blue with full alpha
        let blue = pack_color([0.0, 0.0, 1.0, 1.0]);
        assert_eq!(blue, 0xFFFF0000);

        // Transparent black
        let transparent = pack_color([0.0, 0.0, 0.0, 0.0]);
        assert_eq!(transparent, 0x00000000);

        // 50% gray with 50% alpha
        let gray = pack_color([0.5, 0.5, 0.5, 0.5]);
        // 0.5 * 255 = 127.5 -> 127 (truncation)
        assert_eq!(gray, 0x7F7F7F7F);
    }

    #[test]
    fn test_gpu_ball_data_size() {
        // Verify the struct size matches what WGSL expects
        // position: vec2<f32> = 8 bytes
        // radius: f32 = 4 bytes
        // color: u32 = 4 bytes
        // Total: 16 bytes
        assert_eq!(std::mem::size_of::<GpuBallData>(), 16);
    }

    #[test]
    fn test_ball_uniforms_size() {
        // view_proj: mat4x4<f32> = 64 bytes
        // screen_size: vec2<f32> = 8 bytes
        // eq_edge_distance: f32 = 4 bytes
        // cutoff_distance: f32 = 4 bytes
        // show_cutoff_rings: u32 = 4 bytes
        // _padding: [u32; 3] = 12 bytes
        // Total: 96 bytes
        assert_eq!(std::mem::size_of::<BallUniforms>(), 96);
    }
}
