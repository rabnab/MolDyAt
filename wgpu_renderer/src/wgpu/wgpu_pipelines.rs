//! Pipeline collection for WGPU rendering.
//!
//! Wraps a [`RenderGraph`] with named handles so callers can still
//! reference specific pipelines (e.g., cycling ring display mode on the
//! ball pipeline).

use crate::wgpu::gpu::geometry::build_view_proj_matrix;
use crate::wgpu::pipelines::ball_pipeline::BallPipeline;
use crate::wgpu::pipelines::crosshair_pipeline::CrosshairPipeline;
use crate::wgpu::pipelines::endpoint_pipeline::EndpointPipeline;
use crate::wgpu::pipelines::frame_pipeline::FramePipeline;
use crate::wgpu::pipelines::grid_pipeline::GridPipeline;
use crate::wgpu::pipelines::line_pipeline::LinePipeline;
use crate::wgpu::pipelines::sidebar_pipeline::SidebarPipeline;
use crate::wgpu::pipelines::source_label_pipeline::SourceLabelPipeline;
use crate::wgpu::pipelines::source_sphere_pipeline::SourceSpherePipeline;
use crate::wgpu::pipelines::text_pipeline::TextPipeline;
use crate::wgpu::pipelines::thermo_heatmap_pipeline::ThermoHeatmapPipeline;
use crate::wgpu::pipelines::thermo_value_pipeline::ThermoValueOverlayPipeline;
use crate::wgpu::pipelines::thermostat_label_pipeline::ThermostatLabelPipeline;
use crate::wgpu::pipelines::thermostat_pipeline::ThermostatPipeline;
use crate::wgpu::pipelines::valve_pipeline::ValvePipeline;
use crate::wgpu::pipelines::wall_pipeline::WallPipeline;
use crate::wgpu::render_graph::{PipelineHandle, PrepareContext, RenderGraph, RenderPipeline};
use crate::wgpu::wgpu_context::WgpuContext;
use rust_thermo_core::application::AppResult;
use rust_thermo_core::application::renderer_snapshot::{CellGridSpec, RendererSnapshot};
use rust_thermo_core::graphics::renderer::Margins;
use rust_thermo_core::graphics::renderer::event_loop::FramePreviews;
use rust_thermo_core::graphics::renderer::state_layers::ThermoDisplayMode;

/// Named handles for direct pipeline access.
pub struct PipelineHandles {
    pub sidebar: PipelineHandle,
    pub frame: PipelineHandle,
    pub grid: PipelineHandle,
    pub thermo_heatmap: PipelineHandle,
    pub thermo_value_overlay: PipelineHandle,
    pub wall: PipelineHandle,
    pub valve: PipelineHandle,
    pub line: PipelineHandle,
    pub endpoint: PipelineHandle,
    pub ball: PipelineHandle,
    pub crosshair: PipelineHandle,
    pub text: PipelineHandle,
    pub source_sphere: PipelineHandle,
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
    /// Every pipeline is built the same way (`new(device, queue, format)`) and
    /// registered in execution order (later pipelines draw on top). Each
    /// pipeline picks its own viewport (full window or inset simulation pane).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let mut graph = RenderGraph::new();
        let mut add = |pipeline: Box<dyn RenderPipeline>| graph.add_pipeline(pipeline);

        let sidebar = add(Box::new(SidebarPipeline::new(device, queue, format)));
        let frame = add(Box::new(FramePipeline::new(device, queue, format)));
        let grid = add(Box::new(GridPipeline::new(device, queue, format)));
        let thermo_heatmap = add(Box::new(ThermoHeatmapPipeline::new(device, queue, format)));
        let thermo_value_overlay = add(Box::new(ThermoValueOverlayPipeline::new(
            device, queue, format,
        )));
        let wall = add(Box::new(WallPipeline::new(device, queue, format)));
        let valve = add(Box::new(ValvePipeline::new(device, queue, format)));
        let line = add(Box::new(LinePipeline::new(device, queue, format)));
        let endpoint = add(Box::new(EndpointPipeline::new(device, queue, format)));
        add(Box::new(ThermostatPipeline::new(device, queue, format)));
        let source_sphere = add(Box::new(SourceSpherePipeline::new(device, queue, format)));
        let ball = add(Box::new(BallPipeline::new(device, queue, format)));
        let crosshair = add(Box::new(CrosshairPipeline::new(device, queue, format)));
        add(Box::new(ThermostatLabelPipeline::new(
            device, queue, format,
        )));
        add(Box::new(SourceLabelPipeline::new(device, queue, format)));
        let text = add(Box::new(TextPipeline::new(device, queue, format)));

        log::debug!(
            "{} pipelines registered in RenderGraph",
            graph.pipeline_count()
        );

        Self {
            graph,
            handles: PipelineHandles {
                sidebar,
                frame,
                grid,
                thermo_heatmap,
                thermo_value_overlay,
                wall,
                valve,
                line,
                endpoint,
                ball,
                crosshair,
                text,
                source_sphere,
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
        show_ball_outline: bool,
        thermo_display: ThermoDisplayMode,
        margin: Margins,
        operator_previews: FramePreviews,
    ) -> AppResult {
        let window_size = ctx.size();

        let prepare_ctx = PrepareContext {
            device: &ctx.device,
            queue: &ctx.queue,
            snapshot,
            view_proj,
            cursor_world_pos,
            current_ball_radius,
            show_ball_outline,
            window_size,
            margin,
            thermo_display,
            operator_previews,
        };

        self.prepare(&prepare_ctx)
    }

    /// Prepare all enabled pipelines from an already-built context.
    pub(crate) fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        self.graph.prepare_all(ctx)
    }

    /// Render all enabled pipelines into a render pass.
    ///
    /// The graph handles viewport switching automatically based on each
    /// pipeline's [`ViewportMode`](crate::wgpu::render_graph::ViewportMode).
    pub fn render_all<'rp>(
        &'rp self,
        render_pass: &mut wgpu::RenderPass<'rp>,
        ctx: &WgpuContext,
        margin: Margins,
    ) {
        self.render(render_pass, ctx.size(), margin);
    }

    /// Render all enabled pipelines for a target of `window_size` pixels.
    pub(crate) fn render<'rp>(
        &'rp self,
        render_pass: &mut wgpu::RenderPass<'rp>,
        window_size: (u32, u32),
        margin: Margins,
    ) {
        self.graph.render_all(render_pass, window_size, margin);
    }

    /// Cycle ring display mode on the ball pipeline (applied on the next frame).
    pub fn cycle_ring_display_mode(&mut self) {
        if let Some(ball) = self
            .graph
            .get_pipeline_mut::<BallPipeline>(self.handles.ball)
        {
            ball.cycle_ring_display_mode();
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
