//! Grid rendering pipeline for WGPU
//!
//! Renders a checkerboard grid background using instanced quads.
//! Each cell is drawn as a separate instance for efficient GPU rendering.

use crate::application::renderer_snapshot::CellGridSpec;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Uniform buffer data for grid rendering.
/// Must match the layout in grid.wgsl exactly.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GridUniforms {
    /// 4x4 view-projection matrix (world -> NDC)
    pub view_proj: [[f32; 4]; 4],
    /// Grid origin (top-left) in world coordinates
    pub grid_origin: [f32; 2],
    /// Cell size (width, height) in world coordinates
    pub grid_cell_size: [f32; 2],
    /// Grid dimensions (cols, rows) - using u32 as vec2<u32> in shader
    pub grid_dims: [u32; 2],
    /// Padding to satisfy WGPU uniform buffer (std140-like) layout:
    /// ensures `color_even` / `color_odd` (vec4<f32>) start at 16-byte boundaries.
    pub _padding: [u32; 2],
    /// Color for even cells (RGBA)
    pub color_even: [f32; 4],
    /// Color for odd cells (RGBA)
    pub color_odd: [f32; 4],
}

impl Default for GridUniforms {
    fn default() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            grid_origin: [0.0, 0.0],
            grid_cell_size: [1.0, 1.0],
            grid_dims: [10, 10],
            _padding: [0, 0],
            // Light gray for even cells
            color_even: [0.15, 0.15, 0.18, 1.0],
            // Slightly darker for odd cells
            color_odd: [0.12, 0.12, 0.15, 1.0],
        }
    }
}

/// Grid rendering pipeline and resources.
pub struct GridPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    uniforms: GridUniforms,
}

impl GridPipeline {
    /// Create a new grid pipeline.
    ///
    /// # Arguments
    /// * `device` - WGPU device
    /// * `format` - Surface texture format
    /// * `shader_source` - WGSL shader source code
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, shader_source: &str) -> Self {
        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        // Create uniform buffer
        let uniforms = GridUniforms::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grid Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Grid Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
            label: Some("Grid Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Grid Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grid Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[], // No vertex buffers - using instancing
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
            bind_group,
            uniforms,
        }
    }

    /// Update uniforms from grid spec and viewport.
    ///
    /// # Arguments
    /// * `queue` - WGPU queue for buffer writes
    /// * `grid_spec` - Grid layout specification
    /// * `view_proj` - View-projection matrix (world -> NDC)
    pub fn update_uniforms(
        &mut self,
        queue: &wgpu::Queue,
        grid_spec: &CellGridSpec,
        view_proj: [[f32; 4]; 4],
    ) {
        self.uniforms.view_proj = view_proj;
        self.uniforms.grid_origin = [grid_spec.origin_x, grid_spec.origin_y];
        self.uniforms.grid_cell_size = [grid_spec.cell_width, grid_spec.cell_height];
        self.uniforms.grid_dims = [grid_spec.cols as u32, grid_spec.rows as u32];

        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniforms]),
        );
    }

    /// Set grid colors.
    ///
    /// # Arguments
    /// * `queue` - WGPU queue for buffer writes
    /// * `even` - RGBA color for even cells
    /// * `odd` - RGBA color for odd cells
    pub fn set_colors(&mut self, queue: &wgpu::Queue, even: [f32; 4], odd: [f32; 4]) {
        self.uniforms.color_even = even;
        self.uniforms.color_odd = odd;
        // NOTE: This writes the entire uniform buffer even though only colors changed.
        // This approach is used for simplicity and is subject to later improvement.
        // A more efficient implementation would use queue.write_buffer with a calculated
        // offset and size to update only the color fields.
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniforms]),
        );
    }

    /// Render the grid.
    ///
    /// # Arguments
    /// * `render_pass` - Active render pass
    /// * `instance_count` - Number of cells to draw (rows * cols)
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, instance_count: u32) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        // 6 vertices per quad (2 triangles), instanced for each cell
        render_pass.draw(0..6, 0..instance_count);
    }

    /// Get the total number of cells from current uniforms.
    pub fn cell_count(&self) -> u32 {
        self.uniforms.grid_dims[0] * self.uniforms.grid_dims[1]
    }
}

/// Build a view-projection matrix for 2D rendering.
///
/// Converts world coordinates to NDC (-1..1) based on viewport settings.
/// The view is centered on `(center_x, center_y)` in world coordinates.
///
/// # Arguments
/// * `world_width` - Width of visible world region (before zoom)
/// * `world_height` - Height of visible world region (before zoom)
/// * `center_x` - X coordinate of view center in world space
/// * `center_y` - Y coordinate of view center in world space
/// * `zoom` - Zoom factor (1.0 = no zoom, >1 = zoom in)
pub fn build_view_proj_matrix(
    world_width: f32,
    world_height: f32,
    center_x: f32,
    center_y: f32,
    zoom: f32,
) -> [[f32; 4]; 4] {
    // Orthographic projection: maps world coords to NDC
    // NDC: x in [-1, 1], y in [-1, 1]
    // Visible world region is centered at (center_x, center_y)

    let effective_width = world_width / zoom;
    let effective_height = world_height / zoom;

    // Scale: 2.0 / width maps width to [-1, 1]
    let scale_x = 2.0 / effective_width;
    let scale_y = 2.0 / effective_height;

    // Translation: move (center_x, center_y) to origin (NDC 0,0)
    // Combined matrix (column-major for WGPU)
    [
        [scale_x, 0.0, 0.0, 0.0],
        [0.0, scale_y, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-center_x * scale_x, -center_y * scale_y, 0.0, 1.0],
    ]
}
