//! Crosshair rendering pipeline for WGPU
//!
//! Renders a crosshair at the cursor position and optionally a ball outline
//! when the user is placing a ball (Ctrl held).

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Uniform buffer data for crosshair rendering.
/// Must match the `Uniforms` struct in crosshair.wgsl exactly.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct CrosshairUniforms {
    /// 4x4 view-projection matrix (world -> NDC)
    pub view_proj: [[f32; 4]; 4],
    /// Cursor position in world coordinates
    pub cursor_pos: [f32; 2],
    /// Ball radius to show (for outline)
    pub ball_radius: f32,
    /// Show ball outline flag (1.0 = show, 0.0 = hide)
    pub show_ball_outline: f32,
    /// Equilibrium radius for ball outline
    pub eq_radius: f32,
    /// Cutoff radius for ball outline
    pub cutoff_radius: f32,
    /// Padding for alignment
    pub _padding: [f32; 2],
}

impl Default for CrosshairUniforms {
    fn default() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            cursor_pos: [0.0, 0.0],
            ball_radius: 0.6,
            show_ball_outline: 0.0,
            eq_radius: 0.88,
            cutoff_radius: 4.88,
            _padding: [0.0, 0.0],
        }
    }
}

/// Crosshair rendering pipeline and resources.
pub struct CrosshairPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    uniforms: CrosshairUniforms,
}

impl CrosshairPipeline {
    /// Create a new crosshair pipeline.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, shader_source: &str) -> Self {
        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Crosshair Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        // Create uniform buffer
        let uniforms = CrosshairUniforms::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Crosshair Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Crosshair Bind Group Layout"),
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
            label: Some("Crosshair Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Crosshair Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        // Create render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Crosshair Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
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
            bind_group,
            uniforms,
        }
    }

    /// Update the crosshair uniforms.
    #[allow(clippy::too_many_arguments)]
    pub fn update_uniforms(
        &mut self,
        queue: &wgpu::Queue,
        view_proj: [[f32; 4]; 4],
        cursor_pos: [f32; 2],
        ball_radius: f32,
        show_ball_outline: bool,
        eq_edge_distance: f32,
        cutoff_distance: f32,
    ) {
        self.uniforms.view_proj = view_proj;
        self.uniforms.cursor_pos = cursor_pos;
        self.uniforms.ball_radius = ball_radius;
        self.uniforms.show_ball_outline = if show_ball_outline { 1.0 } else { 0.0 };
        // Calculate eq_radius and cutoff_radius like in ball shader
        self.uniforms.eq_radius = ball_radius + eq_edge_distance * 0.5;
        self.uniforms.cutoff_radius = self.uniforms.eq_radius + cutoff_distance;
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniforms]),
        );
    }

    /// Render the crosshair (and optionally ball outline).
    pub fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        // Instance 0: crosshair (4 quads = 24 vertices)
        // Instance 1: ball outline (1 quad = 6 vertices)
        render_pass.draw(0..24, 0..1); // Crosshair
        if self.uniforms.show_ball_outline > 0.5 {
            render_pass.draw(0..6, 1..2); // Ball outline
        }
    }
}

// ── RenderPipeline trait implementation ─────────────────────────────

use super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use rust_thermo_core::application::AppResult;

impl RenderPipeline for CrosshairPipeline {
    fn name(&self) -> &'static str {
        "crosshair"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::FullWindow
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        // Remap the inset view_proj to full-window NDC so the crosshair
        // renders across the entire window including the sidebar margin.
        let (fw, fh) = (ctx.window_size.0 as f32, ctx.window_size.1 as f32);
        let vp_w = (fw - ctx.margin.horizontal()).max(1.0);
        let vp_h = (fh - ctx.margin.vertical()).max(1.0);
        let sx = vp_w / fw;
        let sy = vp_h / fh;
        let ox = (ctx.margin.left - ctx.margin.right) / fw;
        let oy = (ctx.margin.bottom - ctx.margin.top) / fh;
        // Column-major remap matrix: scale then translate
        let remap: [[f32; 4]; 4] = [
            [sx, 0.0, 0.0, 0.0],
            [0.0, sy, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [ox, oy, 0.0, 1.0],
        ];
        let full_vp = mat4_mul(&remap, &ctx.view_proj);
        self.update_uniforms(
            ctx.queue,
            full_vp,
            [ctx.cursor_world_pos.0, ctx.cursor_world_pos.1],
            ctx.current_ball_radius,
            ctx.ctrl_pressed,
            ctx.snapshot.equilibrium_edge_distance,
            ctx.snapshot.cutoff_distance,
        );
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.draw(render_pass);
    }
}

/// Multiply two column-major 4×4 matrices: result = a * b.
fn mat4_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            out[col][row] = (0..4).map(|k| a[k][row] * b[col][k]).sum();
        }
    }
    out
}
