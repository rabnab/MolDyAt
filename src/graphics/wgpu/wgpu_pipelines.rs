//! Pipeline collection for WGPU rendering
//!
//! Groups all rendering pipelines together to reduce parameter passing
//! and simplify the main window code.

use super::ball_pipeline::BallPipeline;
use super::crosshair_pipeline::CrosshairPipeline;
use super::frame_pipeline::FramePipeline;
use super::grid_pipeline::{GridPipeline, build_view_proj_matrix};
use super::line_pipeline::LinePipeline;
use super::text_pipeline::TextPipeline;
use super::wgpu_context::WgpuContext;
use crate::application::renderer_snapshot::{CellGridSpec, RendererSnapshot};
use crate::graphics::window::{MARGIN_PX_X, MARGIN_PX_Y};

/// Shader source code (embedded at compile time)
const GRID_SHADER: &str = include_str!("shaders/grid.wgsl");
const BALL_SHADER: &str = include_str!("shaders/ball.wgsl");
const CROSSHAIR_SHADER: &str = include_str!("shaders/crosshair.wgsl");
const LINE_SHADER: &str = include_str!("shaders/line.wgsl");
const FRAME_SHADER: &str = include_str!("shaders/frame.wgsl");

/// Collection of all rendering pipelines.
///
/// This struct groups the pipelines together to:
/// - Reduce the number of parameters passed around
/// - Provide a single place to initialize all pipelines
/// - Encapsulate pipeline update and render logic
pub struct WgpuPipelines {
    pub grid: GridPipeline,
    pub ball: BallPipeline,
    pub line: LinePipeline,
    pub crosshair: CrosshairPipeline,
    pub frame: FramePipeline,
    pub text: TextPipeline,
}

impl WgpuPipelines {
    /// Create all rendering pipelines.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        println!("Initializing rendering pipelines...");

        let grid = GridPipeline::new(device, format, GRID_SHADER);
        println!("✓ Grid pipeline initialized");

        let frame = FramePipeline::new(device, format, FRAME_SHADER);
        println!("✓ Frame pipeline initialized");

        let ball = BallPipeline::new(device, format, BALL_SHADER);
        println!("✓ Ball pipeline initialized");

        let line = LinePipeline::new(device, format, LINE_SHADER);
        println!("✓ Line pipeline initialized");

        let crosshair = CrosshairPipeline::new(device, format, CROSSHAIR_SHADER);
        println!("✓ Crosshair pipeline initialized");

        let text = TextPipeline::new(device, format);
        println!("✓ Text pipeline initialized");

        Self {
            grid,
            ball,
            line,
            crosshair,
            frame,
            text,
        }
    }

    /// Update all pipeline uniforms from snapshot data.
    ///
    /// # Arguments
    /// * `ctx` - WGPU context with device and queue
    /// * `snapshot` - Current renderer snapshot with ball and grid data
    /// * `view_proj` - View-projection matrix
    /// * `cursor_world_pos` - Cursor position in world coordinates
    /// * `current_ball_radius` - Current ball placement radius
    /// * `ctrl_pressed` - Whether Ctrl key is pressed (shows ball outline)
    #[allow(clippy::too_many_arguments)]
    pub fn update_from_snapshot(
        &mut self,
        ctx: &WgpuContext,
        snapshot: &RendererSnapshot,
        view_proj: [[f32; 4]; 4],
        cursor_world_pos: (f32, f32),
        current_ball_radius: f32,
        ctrl_pressed: bool,
    ) {
        let grid_spec = &snapshot.grid_spec;
        let eq_edge_distance = snapshot.equilibrium_edge_distance;
        let cutoff_distance = snapshot.cutoff_distance;
        let (win_width, win_height) = ctx.size();

        // Update grid
        self.grid.update_uniforms(&ctx.queue, grid_spec, view_proj);

        // Update balls
        self.ball.update_uniforms(
            &ctx.queue,
            view_proj,
            [win_width as f32, win_height as f32],
            eq_edge_distance,
            cutoff_distance,
        );
        self.ball
            .upload_balls(&ctx.device, &ctx.queue, &snapshot.balls);

        // Update neighbor lines
        self.line.update_uniforms(&ctx.queue, view_proj);
        self.line
            .upload_neighbor_lines(&ctx.device, &ctx.queue, &snapshot.balls);

        // Update crosshair
        self.crosshair.update_uniforms(
            &ctx.queue,
            view_proj,
            [cursor_world_pos.0, cursor_world_pos.1],
            current_ball_radius,
            ctrl_pressed,
            eq_edge_distance,
            cutoff_distance,
        );

        // Update cursor text
        let margin = (MARGIN_PX_X, MARGIN_PX_Y);
        let cursor_text = format!("({:.2}, {:.2})", cursor_world_pos.0, cursor_world_pos.1);
        self.text.update_text(
            &ctx.device,
            &ctx.queue,
            &cursor_text,
            (win_width, win_height),
            margin,
        );
    }

    /// Render all pipelines to a render pass.
    ///
    /// Order: frame border -> grid -> neighbor lines -> balls -> crosshair -> text
    pub fn render<'a>(
        &'a mut self,
        render_pass: &mut wgpu::RenderPass<'a>,
        ctx: &WgpuContext,
        margin: (f32, f32),
    ) {
        let (width, height) = ctx.size();

        // Draw frame border in full-window viewport
        self.frame
            .update_vertices(&ctx.queue, width as f32, height as f32, margin);
        render_pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
        self.frame.render(render_pass);

        // Set viewport with margins to create inset simulation pane
        let viewport_x = margin.0;
        let viewport_y = margin.1;
        let viewport_width = (width as f32 - 2.0 * margin.0).max(1.0);
        let viewport_height = (height as f32 - 2.0 * margin.1).max(1.0);
        render_pass.set_viewport(
            viewport_x,
            viewport_y,
            viewport_width,
            viewport_height,
            0.0,
            1.0,
        );

        // Draw grid
        let cell_count = self.grid.cell_count();
        self.grid.render(render_pass, cell_count);

        // Draw neighbor lines (before balls so lines appear behind)
        self.line.render(render_pass);

        // Draw balls
        self.ball.render(render_pass);

        // Draw crosshair
        self.crosshair.render(render_pass);

        // Draw cursor position text in margin (screen space)
        render_pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
        self.text.render(render_pass);
    }
}

/// Build view-projection matrix from grid spec and viewport state.
pub fn compute_view_proj(
    grid_spec: &CellGridSpec,
    pan_x: f32,
    pan_y: f32,
    zoom: f32,
) -> [[f32; 4]; 4] {
    let world_width = grid_spec.cols as f32 * grid_spec.cell_width;
    let world_height = grid_spec.rows as f32 * grid_spec.cell_height;
    build_view_proj_matrix(world_width, world_height, pan_x, pan_y, zoom)
}
