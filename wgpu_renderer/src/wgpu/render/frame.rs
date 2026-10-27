// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! One frame of the main window, drawn without a window (#372): prepare the
//! pipelines from the render state's snapshot, acquire the surface texture,
//! record, submit and present. What the window has to do afterwards
//! (configure the surface for its size, ask for another redraw, exit) comes
//! back as a [`FrameOutcome`] for the shell.

use super::wgpu_context::WgpuContext;
use super::wgpu_pipelines::{WgpuPipelines, compute_view_proj};
use std::time::{Duration, Instant};
use thermo_runtime::application::scene_layers::SceneLayers;
use thermo_ui::graphics::renderer::Margins;
use thermo_ui::graphics::renderer::event_loop::{OperatorTool, UnifiedRenderState};

/// Log target for the timings of frames that apply a window resize. Enable
/// with `RUST_LOG=info,moldyat::resize=debug` (issue #217).
const RESIZE_TIMING: &str = "moldyat::resize";

/// Where a frame's time went, from acquiring the surface texture on.
struct FrameTimings {
    /// Waiting for a surface texture (blocks on vsync with a full queue).
    acquire: Duration,
    /// Recording, submitting and presenting the frame.
    submit_present: Duration,
    /// The surface no longer matches the window; configure it again.
    suboptimal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurfaceErrorAction {
    Exit,
    Skip,
    Reconfigure,
    RetryLater,
}

/// What the shell does after a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameOutcome {
    /// Drawn, or nothing to draw (rendering off, no snapshot yet).
    Done,
    /// Drawn, but the surface no longer fits the window: configure it for
    /// the window's size now.
    Suboptimal,
    /// Not drawn: configure the surface for the window's size and redraw.
    Reconfigure,
    /// Not drawn this time (skipped while hidden, or to be retried): redraw
    /// when `retry` is set.
    NotDrawn { retry: bool },
    /// The surface cannot be drawn to: exit.
    Exit,
}

/// How to react to a frame that could not be acquired. `Success` and
/// `Suboptimal` carry a texture and are rendered, so they never get here.
pub(crate) fn surface_error_action(status: &wgpu::CurrentSurfaceTexture) -> SurfaceErrorAction {
    match status {
        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
            SurfaceErrorAction::Reconfigure
        }
        wgpu::CurrentSurfaceTexture::Occluded => SurfaceErrorAction::Skip,
        // Repeats on every attempt, so retrying would only spin.
        wgpu::CurrentSurfaceTexture::Validation => SurfaceErrorAction::Exit,
        wgpu::CurrentSurfaceTexture::Timeout
        | wgpu::CurrentSurfaceTexture::Success(_)
        | wgpu::CurrentSurfaceTexture::Suboptimal(_) => SurfaceErrorAction::RetryLater,
    }
}

/// Draw one frame of the main window from `state` into the context's
/// surface. The surface is configured first for a size the shell asked for
/// (`WgpuContext::resize`).
pub(crate) fn draw_main_frame(
    ctx: &mut WgpuContext,
    pipelines: &mut WgpuPipelines,
    state: &mut UnifiedRenderState,
) -> FrameOutcome {
    if !state.should_render() {
        return FrameOutcome::Done;
    }

    state.begin_render();
    let configure_start = Instant::now();
    let resize = ctx.apply_pending_resize();
    let configure = configure_start.elapsed();

    let mut outcome = FrameOutcome::Done;
    if let Some(snapshot) = state.last_snapshot() {
        let prepare_start = Instant::now();
        let pan = state.viewport().pan_offset();
        let zoom = state.viewport().zoom();
        let view_proj = compute_view_proj(&snapshot.grid_spec, pan.x, pan.y, zoom);

        let show_ball_outline = state.active_tool() == OperatorTool::Ball;
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
            // The physics thread's species of new balls, looked up in the
            // world's table (the window's own copy may not know the table).
            snapshot
                .species
                .get(snapshot.cursor_species)
                .map_or(0.0, |s| s.display_radius),
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

        outcome = match render_frame(ctx, pipelines, state.viewport().inset_margins(), layers) {
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
                if timings.suboptimal {
                    FrameOutcome::Suboptimal
                } else {
                    FrameOutcome::Done
                }
            }
            Err(error) => match surface_error_action(&error) {
                SurfaceErrorAction::Exit => {
                    log::error!("Render error: {:?}", error);
                    FrameOutcome::Exit
                }
                SurfaceErrorAction::Skip => {
                    // Redrawn on `WindowEvent::Occluded(false)`.
                    log::debug!("Frame skipped: {:?}", error);
                    FrameOutcome::NotDrawn { retry: false }
                }
                SurfaceErrorAction::Reconfigure => {
                    log::error!("Render error: {:?}", error);
                    FrameOutcome::Reconfigure
                }
                SurfaceErrorAction::RetryLater => {
                    log::error!("Render error: {:?}", error);
                    FrameOutcome::NotDrawn { retry: true }
                }
            },
        };
    }

    state.end_frame();
    outcome
}

/// Record one frame of the prepared `pipelines` into `view`, a target of
/// `size` pixels: clear to black, then every enabled layer. Needs no window:
/// the main window's surface texture and offscreen targets (the GPU smoke
/// tests) are recorded alike.
pub(crate) fn encode_frame(
    device: &wgpu::Device,
    pipelines: &WgpuPipelines,
    view: &wgpu::TextureView,
    size: (u32, u32),
    inset: Margins,
    layers: SceneLayers,
) -> wgpu::CommandBuffer {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Render Encoder"),
    });
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Main Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        pipelines.render(&mut render_pass, size, inset, layers);
    }
    encoder.finish()
}

/// Acquire the surface texture, record the frame into it and present it.
fn render_frame(
    ctx: &WgpuContext,
    pipelines: &WgpuPipelines,
    inset: Margins,
    layers: SceneLayers,
) -> Result<FrameTimings, wgpu::CurrentSurfaceTexture> {
    let acquire_start = Instant::now();
    let (output, suboptimal) = match ctx.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
        wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
        status => return Err(status),
    };
    let acquire = acquire_start.elapsed();
    let submit_start = Instant::now();
    let view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let commands = encode_frame(&ctx.device, pipelines, &view, ctx.size(), inset, layers);
    ctx.queue.submit(std::iter::once(commands));
    ctx.queue.present(output);

    Ok(FrameTimings {
        acquire,
        submit_present: submit_start.elapsed(),
        suboptimal,
    })
}
