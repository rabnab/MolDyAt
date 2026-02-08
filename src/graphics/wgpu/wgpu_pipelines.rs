//! Pipeline collection for WGPU rendering.
//!
//! Wraps a [`RenderGraph`] with named handles so callers can still
//! reference specific pipelines (e.g., toggling cutoff rings on the
//! ball pipeline).

use super::ball_pipeline::BallPipeline;
use super::crosshair_pipeline::CrosshairPipeline;
use super::frame_pipeline::FramePipeline;
use super::grid_pipeline::{GridPipeline, build_view_proj_matrix};
use super::line_pipeline::LinePipeline;
use super::render_graph::{PipelineHandle, PrepareContext, RenderGraph};
use super::text_pipeline::TextPipeline;
use super::wgpu_context::WgpuContext;
use crate::application::AppResult;
use crate::application::renderer_snapshot::{CellGridSpec, RendererSnapshot};
use crate::graphics::window::{MARGIN_PX_X, MARGIN_PX_Y};

/// Shader source code (embedded at compile time).
const GRID_SHADER: &str = include_str!("shaders/grid.wgsl");
const BALL_SHADER: &str = include_str!("shaders/ball.wgsl");
const CROSSHAIR_SHADER: &str = include_str!("shaders/crosshair.wgsl");
const LINE_SHADER: &str = include_str!("shaders/line.wgsl");
const FRAME_SHADER: &str = include_str!("shaders/frame.wgsl");

/// Named handles for direct pipeline access.
pub struct PipelineHandles {
    pub frame: PipelineHandle,
    pub grid: PipelineHandle,
    pub line: PipelineHandle,
    pub ball: PipelineHandle,
    pub crosshair: PipelineHandle,
    pub text: PipelineHandle,
}

/// Collection of all rendering pipelines, backed by a [`RenderGraph`].
///
/// Provides:
/// - Graph-based `prepare` / `render` for the common path
/// - Named handles for pipeline-specific operations (e.g., cutoff rings)
pub struct WgpuPipelines {
    graph: RenderGraph,
    pub handles: PipelineHandles,
}

impl WgpuPipelines {
    /// Create all rendering pipelines and register them in the graph.
    ///
    /// Execution order: frame → grid → lines → balls → crosshair → text.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        println!("Initializing rendering pipelines...");

        let mut graph = RenderGraph::new();

        // Register pipelines in execution order.
        // Frame renders in FullWindow viewport; grid/lines/balls/crosshair
        // render in Inset viewport; text renders in FullWindow viewport.
        let frame = graph.add_pipeline(Box::new(FramePipeline::new(device, format, FRAME_SHADER)));
        let grid = graph.add_pipeline(Box::new(GridPipeline::new(device, format, GRID_SHADER)));
        let line = graph.add_pipeline(Box::new(LinePipeline::new(device, format, LINE_SHADER)));
        let ball = graph.add_pipeline(Box::new(BallPipeline::new(device, format, BALL_SHADER)));
        let crosshair = graph.add_pipeline(Box::new(CrosshairPipeline::new(
            device,
            format,
            CROSSHAIR_SHADER,
        )));
        let text = graph.add_pipeline(Box::new(TextPipeline::new(device, format)));

        println!(
            "All {} pipelines registered in RenderGraph",
            graph.pipeline_count()
        );

        Self {
            graph,
            handles: PipelineHandles {
                frame,
                grid,
                line,
                ball,
                crosshair,
                text,
            },
        }
    }

    /// Prepare all pipelines from snapshot data.
    ///
    /// Delegates to [`RenderGraph::prepare_all`] with a [`PrepareContext`]
    /// built from the provided arguments.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_from_snapshot(
        &mut self,
        ctx: &WgpuContext,
        snapshot: &RendererSnapshot,
        view_proj: [[f32; 4]; 4],
        cursor_world_pos: (f32, f32),
        current_ball_radius: f32,
        ctrl_pressed: bool,
    ) -> AppResult {
        let window_size = ctx.size();
        let margin = (MARGIN_PX_X, MARGIN_PX_Y);

        let prepare_ctx = PrepareContext {
            device: &ctx.device,
            queue: &ctx.queue,
            snapshot,
            view_proj,
            cursor_world_pos,
            current_ball_radius,
            ctrl_pressed,
            window_size,
            margin,
        };

        self.graph.prepare_all(&prepare_ctx)
    }

    /// Render all enabled pipelines into a render pass.
    ///
    /// The graph handles viewport switching automatically based on each
    /// pipeline's [`ViewportMode`](super::render_graph::ViewportMode).
    pub fn render_all<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>, ctx: &WgpuContext) {
        let margin = (MARGIN_PX_X, MARGIN_PX_Y);
        self.graph.render_all(render_pass, ctx.size(), margin);
    }

    /// Toggle cutoff ring visibility on the ball pipeline.
    pub fn toggle_cutoff_rings(&mut self, queue: &wgpu::Queue) {
        if let Some(ball) = self
            .graph
            .get_pipeline_mut::<BallPipeline>(self.handles.ball)
        {
            ball.toggle_cutoff_rings(queue);
        }
    }

    /// Enable or disable a pipeline by handle.
    pub fn set_enabled(&mut self, handle: PipelineHandle, enabled: bool) {
        self.graph.set_enabled(handle, enabled);
    }

    /// Check whether a pipeline is enabled.
    pub fn is_enabled(&self, handle: PipelineHandle) -> bool {
        self.graph.is_enabled(handle)
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
