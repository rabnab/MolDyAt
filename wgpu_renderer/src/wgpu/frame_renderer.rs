// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::wgpu_context::WgpuContext;
use super::wgpu_pipelines::{WgpuPipelines, compute_view_proj};
use crate::wgpu::margins_for_scale_factor;
use rust_thermo_core::application::scene_layers::SceneLayers;
use rust_thermo_core::graphics::renderer::event_loop::UnifiedRenderState;
use rust_thermo_core::objects2d::ball_impl::BallFactory;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

/// Log target for the timings of frames that apply a window resize. Enable
/// with `RUST_LOG=info,resize_timing=debug` (issue #217).
const RESIZE_TIMING: &str = "resize_timing";

/// Coordinates redraw handling and frame submission for the WGPU backend.
pub(super) struct FrameRenderer;

/// Where a frame's time went, from acquiring the surface texture on.
struct FrameTimings {
    /// Waiting for a surface texture (blocks on vsync with a full queue).
    acquire: Duration,
    /// Recording, submitting and presenting the frame.
    submit_present: Duration,
}

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
        state.energy_manager_mut().request_redraw();
    }

    /// Ask the OS for the window size the render state wants (a loaded world
    /// of another shape). When the OS answers at once, no `Resized` event may
    /// follow, so the answer goes through the same resize handling here; the
    /// window may also end up another size than asked (maximised, tiled), in
    /// which case the normal resize path fits the world to it.
    pub(super) fn apply_window_size_request(
        state: &mut UnifiedRenderState,
        window: &Window,
        ctx: &mut WgpuContext,
    ) {
        let Some((width, height)) = state.take_requested_window_size() else {
            return;
        };
        if let Some(size) = window.request_inner_size(PhysicalSize::new(width, height)) {
            ctx.resize(size.width, size.height);
            let margins = margins_for_scale_factor(window.scale_factor());
            state.apply_window_size_answer(size.width as f32, size.height as f32, margins);
        }
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
        let configure_start = Instant::now();
        let resize = ctx.apply_pending_resize();
        let configure = configure_start.elapsed();

        if let Some(snapshot) = state.last_snapshot() {
            let prepare_start = Instant::now();
            let pan = state.viewport().pan_offset();
            let zoom = state.viewport().zoom();
            let view_proj = compute_view_proj(&snapshot.grid_spec, pan.x, pan.y, zoom);

            let show_ball_outline = state.active_tool()
                == rust_thermo_core::graphics::renderer::event_loop::OperatorTool::Ball;
            let cursor_world_pos = state.cursor_world_pos();
            let cursor_wp = (cursor_world_pos.x, cursor_world_pos.y);
            // Rendering off (F2) draws only the base layer: no ball or thermo
            // work below (see `SceneLayers`).
            let layers = state.frame_layers();
            let ball_count = if layers.contains(SceneLayers::BALLS) {
                snapshot.balls.len() as u32
            } else {
                0
            };
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
                state.viewport().inset_margins(),
                operator_previews,
                layers,
            ) {
                eprintln!("Pipeline prepare error: {:?}", error);
            }

            state.metrics_mut().record_balls_rendered(ball_count);
            for _ in 0..6 {
                state.metrics_mut().record_draw_call(0);
            }

            let prepare = prepare_start.elapsed();

            match Self::render_frame(ctx, pipelines, state.viewport().inset_margins(), layers) {
                Ok(timings) => {
                    if let Some(resize) = resize {
                        log::debug!(
                            target: RESIZE_TIMING,
                            "{}x{}: {} resize event(s), first {:.1?} ago at present | configure {:.1?}, prepare {:.1?}, acquire {:.1?}, submit+present {:.1?}",
                            resize.size.0,
                            resize.size.1,
                            resize.requests,
                            resize.since.elapsed(),
                            configure,
                            prepare,
                            timings.acquire,
                            timings.submit_present,
                        );
                    }
                }
                Err(error) => {
                    log::error!("Render error: {:?}", error);
                    match Self::surface_error_action(&error) {
                        SurfaceErrorAction::Exit => event_loop.exit(),
                        SurfaceErrorAction::Reconfigure => {
                            let size = window.inner_size();
                            ctx.resize(size.width.max(1), size.height.max(1));
                            ctx.apply_pending_resize();
                            window.request_redraw();
                        }
                        SurfaceErrorAction::RetryLater => {
                            window.request_redraw();
                        }
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
        inset: rust_thermo_core::graphics::renderer::Margins,
        layers: SceneLayers,
    ) -> Result<FrameTimings, wgpu::SurfaceError> {
        let acquire_start = Instant::now();
        let output = ctx.surface.get_current_texture()?;
        let acquire = acquire_start.elapsed();
        let submit_start = Instant::now();
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

            pipelines.render_all(&mut render_pass, ctx, inset, layers);
        }

        ctx.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(FrameTimings {
            acquire,
            submit_present: submit_start.elapsed(),
        })
    }
}
