// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The window side of a frame (#372): redraw requests, window sizes and
//! what to do after the renderer drew (or could not draw) a frame
//! ([`draw_main_frame`]).

use crate::wgpu::margins_for_scale_factor;
use crate::wgpu::render::frame::{FrameOutcome, draw_main_frame};
use crate::wgpu::render::wgpu_context::WgpuContext;
use crate::wgpu::render::wgpu_pipelines::WgpuPipelines;
use thermo_ui::graphics::renderer::event_loop::UnifiedRenderState;
use winit::dpi::PhysicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

/// What the window does after the renderer's frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct AfterFrame {
    /// Configure the surface for the window's size now.
    pub(crate) fit_surface: bool,
    /// Ask for another redraw.
    pub(crate) redraw: bool,
    pub(crate) exit: bool,
}

/// The window's part after a frame with `outcome`. A frame skipped while
/// the window is hidden asks for nothing: the window is drawn again on
/// `Occluded(false)` (`EventRouter::redraws_main_window`).
pub(crate) fn after_frame(outcome: FrameOutcome) -> AfterFrame {
    match outcome {
        FrameOutcome::Done | FrameOutcome::NotDrawn { retry: false } => AfterFrame::default(),
        FrameOutcome::Suboptimal => AfterFrame {
            fit_surface: true,
            ..AfterFrame::default()
        },
        FrameOutcome::Reconfigure => AfterFrame {
            fit_surface: true,
            redraw: true,
            ..AfterFrame::default()
        },
        FrameOutcome::NotDrawn { retry: true } => AfterFrame {
            redraw: true,
            ..AfterFrame::default()
        },
        FrameOutcome::Exit => AfterFrame {
            exit: true,
            ..AfterFrame::default()
        },
    }
}

/// Coordinates redraw handling for the main window.
pub(crate) struct FrameRenderer;

impl FrameRenderer {
    pub(crate) fn handle_new_snapshot(state: &mut UnifiedRenderState, window: &Window) {
        state.begin_frame();
        window.request_redraw();
        state.energy_manager_mut().request_redraw();
    }

    /// Ask the OS for the window size the render state wants (a loaded world
    /// of another shape), at most the monitor's size and the GPU's largest
    /// surface (#363). When the OS answers at once, no `Resized` event may
    /// follow, so the answer goes through the same resize handling here; the
    /// window may also end up another size than asked (maximised, tiled), in
    /// which case the normal resize path fits the world to it.
    pub(crate) fn apply_window_size_request(
        state: &mut UnifiedRenderState,
        window: &Window,
        ctx: &mut WgpuContext,
    ) {
        let Some((width, height)) =
            state.take_requested_window_size(Self::max_window_size(window, ctx))
        else {
            return;
        };
        if let Some(size) = window.request_inner_size(PhysicalSize::new(width, height)) {
            ctx.resize(size.width, size.height);
            let margins = margins_for_scale_factor(window.scale_factor());
            state.apply_window_size_answer(size.width as f32, size.height as f32, margins);
        }
    }

    /// The largest inner size worth asking for: the current monitor's size
    /// (when known) and the GPU's largest surface.
    fn max_window_size(window: &Window, ctx: &WgpuContext) -> (u32, u32) {
        let max_surface = ctx.max_surface_dimension();
        let (mut width, mut height) = (max_surface, max_surface);
        if let Some(monitor) = window.current_monitor() {
            let size = monitor.size();
            if size.width > 0 && size.height > 0 {
                width = width.min(size.width);
                height = height.min(size.height);
            }
        }
        (width, height)
    }

    /// Handles the `RedrawRequested` event for the main window: the renderer
    /// draws the frame, the window acts on the outcome. Returns `false` if
    /// the event was for another window.
    pub(crate) fn handle_redraw_requested(
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
            "Handling redraw for window {:?}, should_render: {}",
            window_id,
            state.should_render()
        );
        let after = after_frame(draw_main_frame(ctx, pipelines, state));
        if after.fit_surface {
            Self::fit_surface(ctx, window);
        }
        if after.redraw {
            window.request_redraw();
        }
        if after.exit {
            event_loop.exit();
        }
        true
    }

    /// Configure the surface for the window's size now.
    fn fit_surface(ctx: &mut WgpuContext, window: &Window) {
        let size = window.inner_size();
        ctx.resize(size.width.max(1), size.height.max(1));
        ctx.apply_pending_resize();
    }
}
