// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! [`UnifiedRenderState`]'s reactions to the physics thread's results, and
//! fitting the window to a loaded world.

use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::margins::Margins;
use std::time::Instant;
use thermo_runtime::application::PhysicsEvent;

use super::unified_render_state::{PendingWindowRequest, UnifiedRenderState};

impl UnifiedRenderState {
    /// React to a result reported by the physics thread. A loaded world is
    /// shown in the default view of its own rectangle; the view only changes
    /// once the physics thread has actually replaced the world.
    pub(super) fn handle_physics_event(&mut self, event: PhysicsEvent) {
        match event {
            PhysicsEvent::WorldSaved(path) => log::info!("Saved world to {}", path.display()),
            PhysicsEvent::SaveFailed(error) => log::error!("Saving world failed: {error}"),
            PhysicsEvent::WorldLoaded(loaded) => {
                log::info!("{}", loaded.message(true));
                self.reset_tool_handler();
                self.render.viewport.set_world_bounds(loaded.world_rect);
                self.render.viewport.reset();
                self.render.view_dirty = true;
                self.render.pending_window_request = None;
                self.fit_window_to_world();
            }
            PhysicsEvent::LoadFailed(error) => log::error!("Loading world failed: {error}"),
        }
    }

    /// Ask for a window of the world's shape when it differs from the drawing
    /// area's (a loaded world). The world is letterboxed meanwhile, so it is
    /// never stretched; the requested size maps it 1:1 (`LEN_TO_PX`), which
    /// removes the bars. Whatever size the OS grants, the loaded world is
    /// kept as saved (`pending_window_request`). A world of the window's shape is
    /// left alone, which keeps a normal start untouched.
    pub(super) fn fit_window_to_world(&mut self) {
        let viewport = &self.render.viewport;
        let world = viewport.world_bounds();
        let (vp_w, vp_h) = viewport.viewport_size_px();
        let world_aspect = world.w.abs() / world.h.abs().max(f32::EPSILON);
        let viewport_aspect = vp_w / vp_h;
        if (world_aspect / viewport_aspect - 1.0).abs() < ViewportTransform::LETTERBOX_TOLERANCE {
            return;
        }
        let margins = viewport.margins();
        let width = world.w.abs() / ViewportTransform::LEN_TO_PX + margins.horizontal();
        let height = world.h.abs() / ViewportTransform::LEN_TO_PX + margins.vertical();
        self.render.requested_window_size = Some((width.round() as u32, height.round() as u32));
    }

    /// Apply the OS's immediate answer to a window size request: `size` is
    /// what the window got. Unlike a `Resized` event, this answer is final
    /// even when the size did not change, so the loaded world is no longer
    /// waiting for one, and a later (user) resize refits the world as usual.
    pub fn apply_window_size_answer(&mut self, width: f32, height: f32, margin_px: Margins) {
        self.handle_resize(width, height, margin_px);
        self.render.pending_window_request = None;
    }

    /// The window size a backend should request, once (see
    /// [`Self::fit_window_to_world`]).
    /// Taking it means the backend now asks the OS, so the answer is awaited
    /// from here on.
    pub fn take_requested_window_size(&mut self) -> Option<(u32, u32)> {
        let size = self.render.requested_window_size.take()?;
        self.render.pending_window_request = Some(PendingWindowRequest {
            size,
            at: Instant::now(),
        });
        Some(size)
    }
}
