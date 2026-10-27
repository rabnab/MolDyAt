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

/// Coordinates redraw handling for the main window.
pub(crate) struct FrameRenderer;

impl FrameRenderer {
    pub(crate) fn handle_new_snapshot(state: &mut UnifiedRenderState, window: &Window) {
        state.begin_frame();
        window.request_redraw();
        state.energy_manager_mut().request_redraw();
    }

    /// Ask the OS for the window size the render state wants (a loaded world
    /// of another shape). When the OS answers at once, no `Resized` event may
    /// follow, so the answer goes through the same resize handling here; the
    /// window may also end up another size than asked (maximised, tiled), in
    /// which case the normal resize path fits the world to it.
    pub(crate) fn apply_window_size_request(
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
        match draw_main_frame(ctx, pipelines, state) {
            FrameOutcome::Done | FrameOutcome::NotDrawn { retry: false } => {}
            FrameOutcome::Suboptimal => Self::fit_surface(ctx, window),
            FrameOutcome::Reconfigure => {
                Self::fit_surface(ctx, window);
                window.request_redraw();
            }
            FrameOutcome::NotDrawn { retry: true } => window.request_redraw(),
            FrameOutcome::Exit => event_loop.exit(),
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
