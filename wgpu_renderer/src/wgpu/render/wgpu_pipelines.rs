// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pipeline collection for WGPU rendering.
//!
//! Wraps a [`RenderGraph`] with named handles so callers can still
//! reference specific pipelines (e.g., cycling ring display mode on the
//! ball pipeline).

use crate::wgpu::render::gpu::geometry::build_view_proj_matrix;
use crate::wgpu::render::pipelines::ball_pipeline::BallPipeline;
use crate::wgpu::render::pipelines::crosshair_pipeline::CrosshairPipeline;
use crate::wgpu::render::pipelines::endpoint_pipeline::EndpointPipeline;
use crate::wgpu::render::pipelines::frame_pipeline::FramePipeline;
use crate::wgpu::render::pipelines::grid_pipeline::GridPipeline;
use crate::wgpu::render::pipelines::highlight_pipeline::HighlightPipeline;
use crate::wgpu::render::pipelines::line_pipeline::LinePipeline;
use crate::wgpu::render::pipelines::sidebar_pipeline::SidebarPipeline;
use crate::wgpu::render::pipelines::source_label_pipeline::SourceLabelPipeline;
use crate::wgpu::render::pipelines::source_sphere_pipeline::SourceSpherePipeline;
use crate::wgpu::render::pipelines::text_pipeline::TextPipeline;
use crate::wgpu::render::pipelines::thermo_heatmap_pipeline::ThermoHeatmapPipeline;
use crate::wgpu::render::pipelines::thermo_value_pipeline::ThermoValueOverlayPipeline;
use crate::wgpu::render::pipelines::thermostat_label_pipeline::ThermostatLabelPipeline;
use crate::wgpu::render::pipelines::thermostat_pipeline::ThermostatPipeline;
use crate::wgpu::render::pipelines::valve_pipeline::ValvePipeline;
use crate::wgpu::render::pipelines::wall_pipeline::WallPipeline;
use crate::wgpu::render::render_graph::{
    PipelineHandle, PrepareContext, RenderGraph, RenderPipeline,
};
use crate::wgpu::render::wgpu_context::WgpuContext;
use thermo_physics::application::AppResult;
use thermo_runtime::application::renderer_snapshot::{CellGridSpec, RendererSnapshot};
use thermo_runtime::application::scene_layers::SceneLayers;
use thermo_ui::graphics::renderer::Margins;
use thermo_ui::graphics::renderer::event_loop::FramePreviews;
use thermo_ui::graphics::renderer::state_layers::ThermoDisplayMode;

/// Collection of all rendering pipelines, backed by a [`RenderGraph`].
///
/// Provides:
/// - Graph-based `prepare` / `render` for the common path
/// - A handle to the ball pipeline for its display-mode toggle
pub struct WgpuPipelines {
    graph: RenderGraph,
    ball: PipelineHandle,
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

        add(Box::new(SidebarPipeline::new(device, queue, format)));
        add(Box::new(FramePipeline::new(device, queue, format)));
        add(Box::new(GridPipeline::new(device, queue, format)));
        add(Box::new(ThermoHeatmapPipeline::new(device, queue, format)));
        add(Box::new(ThermoValueOverlayPipeline::new(
            device, queue, format,
        )));
        add(Box::new(WallPipeline::new(device, queue, format)));
        add(Box::new(ValvePipeline::new(device, queue, format)));
        add(Box::new(LinePipeline::new(device, queue, format)));
        add(Box::new(EndpointPipeline::new(device, queue, format)));
        add(Box::new(ThermostatPipeline::new(device, queue, format)));
        add(Box::new(SourceSpherePipeline::new(device, queue, format)));
        add(Box::new(HighlightPipeline::new(device, queue, format)));
        let ball = add(Box::new(BallPipeline::new(device, queue, format)));
        add(Box::new(CrosshairPipeline::new(device, queue, format)));
        add(Box::new(ThermostatLabelPipeline::new(
            device, queue, format,
        )));
        add(Box::new(SourceLabelPipeline::new(device, queue, format)));
        add(Box::new(TextPipeline::new(device, queue, format)));

        log::debug!(
            "{} pipelines registered in RenderGraph",
            graph.pipeline_count()
        );

        Self { graph, ball }
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
        inset: Margins,
        operator_previews: FramePreviews,
        layers: SceneLayers,
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
            inset,
            thermo_display,
            operator_previews,
            layers,
        };

        self.prepare(&prepare_ctx)
    }

    /// Prepare all enabled pipelines from an already-built context.
    pub(crate) fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        self.graph.prepare_all(ctx)
    }

    /// Render all enabled pipelines for a target of `window_size` pixels.
    /// The graph handles viewport switching based on each pipeline's
    /// [`ViewportMode`](crate::wgpu::render::render_graph::ViewportMode).
    pub(crate) fn render<'rp>(
        &'rp self,
        render_pass: &mut wgpu::RenderPass<'rp>,
        window_size: (u32, u32),
        inset: Margins,
        layers: SceneLayers,
    ) {
        self.graph
            .render_all(render_pass, window_size, inset, layers);
    }

    /// Cycle ring display mode on the ball pipeline (applied on the next frame).
    pub fn cycle_ring_display_mode(&mut self) {
        if let Some(ball) = self.graph.get_pipeline_mut::<BallPipeline>(self.ball) {
            ball.cycle_ring_display_mode();
        }
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
