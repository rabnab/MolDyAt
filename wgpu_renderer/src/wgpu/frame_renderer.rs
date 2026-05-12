use super::wgpu_context::WgpuContext;
use super::wgpu_pipelines::{WgpuPipelines, compute_view_proj};
use rust_thermo_core::graphics::renderer::event_loop::UnifiedRenderState;
use rust_thermo_core::objects2d::ball_impl::BallFactory;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

/// Coordinates redraw handling and frame submission for the WGPU backend.
pub(super) struct FrameRenderer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SurfaceErrorAction {
    Exit,
    Reconfigure,
    RetryLater,
}

impl FrameRenderer {
    pub(super) fn handle_new_snapshot(state: &mut UnifiedRenderState, window: &Window) {
        state.begin_frame();
        window.request_redraw();
        state.energy_manager_mut().handle_redraw();
    }

    /// Handles the `RedrawRequested` event for the main window, performing rendering if needed.
    /// Returns `true` if the event was handled (i.e., it was for the main window and rendering was attempted), or `false` if the event should be ignored (e.g., it was for a different window).
    pub(super) fn handle_redraw_requested(
        window_id: WindowId,
        event_loop: &ActiveEventLoop,
        window: &Window,
        ctx: &mut WgpuContext,
        pipelines: &mut WgpuPipelines,
        state: &mut UnifiedRenderState,
    ) -> bool {
        if window_id != window.id() {
            return false;
        }

        log::trace!(
            "Handling redraw for  window with id {:?} and should_render: {}",
            window_id,
            state.should_render()
        );

        if !state.should_render() {
            return true;
        }

        state.begin_render();

        if let Some(snapshot) = state.last_snapshot() {
            let pan = state.viewport().pan_offset();
            let zoom = state.viewport().zoom();
            let view_proj = compute_view_proj(&snapshot.grid_spec, pan.x, pan.y, zoom);

            let show_ball_outline = state.active_tool()
                == rust_thermo_core::graphics::renderer::event_loop::OperatorTool::Ball;
            let cursor_world_pos = state.cursor_world_pos();
            let cursor_wp = (cursor_world_pos.x, cursor_world_pos.y);
            let ball_count = snapshot.balls.len() as u32;
            let operator_previews = state.operator_previews();
            if let Err(error) = pipelines.prepare_from_snapshot(
                ctx,
                snapshot,
                view_proj,
                cursor_wp,
                BallFactory::radius_for_size_idx(state.current_ball_size_idx()),
                show_ball_outline,
                state.config().thermo_display,
                state.viewport().margins(),
                operator_previews,
            ) {
                eprintln!("Pipeline prepare error: {:?}", error);
            }

            state.metrics_mut().record_balls_rendered(ball_count);
            for _ in 0..6 {
                state.metrics_mut().record_draw_call(0);
            }

            if let Err(error) = Self::render_frame(ctx, pipelines, state.viewport().margins()) {
                log::error!("Render error: {:?}", error);
                match Self::surface_error_action(&error) {
                    SurfaceErrorAction::Exit => event_loop.exit(),
                    SurfaceErrorAction::Reconfigure => {
                        let size = window.inner_size();
                        ctx.resize(size.width.max(1), size.height.max(1));
                        window.request_redraw();
                    }
                    SurfaceErrorAction::RetryLater => {
                        window.request_redraw();
                    }
                }
            }
        }

        state.end_frame();
        true
    }

    pub(super) fn surface_error_action(error: &wgpu::SurfaceError) -> SurfaceErrorAction {
        match error {
            wgpu::SurfaceError::OutOfMemory => SurfaceErrorAction::Exit,
            wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                SurfaceErrorAction::Reconfigure
            }
            wgpu::SurfaceError::Timeout => SurfaceErrorAction::RetryLater,
            wgpu::SurfaceError::Other => SurfaceErrorAction::RetryLater,
        }
    }

    fn render_frame(
        ctx: &WgpuContext,
        pipelines: &WgpuPipelines,
        margin: rust_thermo_core::graphics::renderer::Margins,
    ) -> Result<(), wgpu::SurfaceError> {
        let output = ctx.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            pipelines.render_all(&mut render_pass, ctx, margin);
        }

        ctx.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
