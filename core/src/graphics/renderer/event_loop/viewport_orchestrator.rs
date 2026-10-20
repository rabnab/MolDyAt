// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::unified_render_state::{PendingWindowRequest, UnifiedRenderState};
use crate::application::WorldCommand;
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::margins::Margins;
use crate::objects2d::{Rect, Vec2D};
use std::time::Duration;

/// How long after asking for a window size a resize of another size still
/// counts as the OS's answer (clamped to the screen) rather than the user's.
const WINDOW_ANSWER_TIMEOUT: Duration = Duration::from_secs(1);

/// Coordinates viewport changes and resize-related physics synchronization.
pub(super) struct ViewportOrchestrator;

impl ViewportOrchestrator {
    pub(super) fn zoom_at_point(state: &mut UnifiedRenderState, world_point: &Vec2D, factor: f32) {
        state.render.viewport.zoom_at_point(world_point, factor);
        state.render.view_dirty = true;
    }

    pub(super) fn reset_view(state: &mut UnifiedRenderState) {
        state.render.viewport.reset();
        state.render.view_dirty = true;
    }

    pub(super) fn handle_resize(
        state: &mut UnifiedRenderState,
        new_width: f32,
        new_height: f32,
        margin_px: Margins,
    ) {
        let viewport = &state.render.viewport;
        if viewport.window_size_px() == (new_width, new_height) && viewport.margins() == margin_px {
            // Nothing changed (e.g. the OS repeats the size we already have):
            // rescaling the world would only add rounding drift.
            return;
        }
        if let Some(request) = state.render.pending_window_request.take()
            && Self::is_answer_to(&request, new_width, new_height)
        {
            // The OS's answer to the window size asked for after a load: the
            // loaded world stays as saved and is fitted into whatever size
            // was granted (letterboxed), instead of being squeezed into a
            // rectangle derived from that size. Any other resize (the user's,
            // or after the answer is overdue) ends the wait and refits the
            // world as usual.
            state.render.viewport.set_margins(margin_px);
            state
                .render
                .viewport
                .on_window_resize((new_width, new_height));
            state.render.view_dirty = true;
            return;
        }
        state.render.viewport.set_margins(margin_px);
        let world_rect = ViewportTransform::compute_world_bounds_from_window_size(
            new_width as u32,
            new_height as u32,
            margin_px,
        );
        Self::handle_resize_with_world_rect(state, new_width, new_height, world_rect);
    }

    /// Whether a resize to `width` x `height` is the OS's answer to
    /// `request`: the requested size (within rounding), or any size soon
    /// after asking (the OS clamped it to the screen or the window manager's
    /// layout). A request is not answered after that, so it can't decide a
    /// later, unrelated resize.
    fn is_answer_to(request: &PendingWindowRequest, width: f32, height: f32) -> bool {
        let (req_w, req_h) = (request.size.0 as f32, request.size.1 as f32);
        let matches = (width - req_w).abs() <= 1.0 && (height - req_h).abs() <= 1.0;
        matches || request.at.elapsed() <= WINDOW_ANSWER_TIMEOUT
    }

    pub(super) fn handle_resize_with_world_rect(
        state: &mut UnifiedRenderState,
        new_width: f32,
        new_height: f32,
        world_rect: Rect,
    ) {
        state
            .physics
            .send_command(WorldCommand::Resize { world_rect });
        state.render.viewport.set_world_bounds(world_rect);
        state
            .render
            .viewport
            .on_window_resize((new_width, new_height));
        state.render.view_dirty = true;
        state.render.resize_pending = true;
    }

    pub(super) fn resume_after_resize_if_pending(state: &mut UnifiedRenderState) {
        if state.render.resize_pending {
            state.render.resize_pending = false;
            state.physics.send_command(WorldCommand::ResumeAfterResize);
        }
    }

    pub(super) fn apply_pan_from_input(state: &mut UnifiedRenderState, delta: Vec2D) {
        let start_offset = state.input.pan_start_offset();
        let new_pan = state.render.viewport.apply_pan_delta(delta, start_offset);
        state.render.viewport.pan(new_pan);
        state.render.view_dirty = true;
    }

    pub(super) fn update_cursor_world_pos(state: &mut UnifiedRenderState, px: &[f32; 2]) {
        let world = state.render.viewport.px_to_world(px);
        state.render.cursor_world_pos = world;
        state.input.set_cursor_pos(world, *px);
    }
}

#[cfg(test)]
mod tests {
    use super::ViewportOrchestrator;
    use crate::application::renderer_snapshot::OperatorSnapshot;
    use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
    use crate::graphics::renderer::ViewportTransform;
    use crate::graphics::renderer::event_loop::energy_window_manager::EnergyWindowManager;
    use crate::graphics::renderer::event_loop::input_handler::{InputHandler, OperatorTool};
    use crate::graphics::renderer::event_loop::metrics::MetricsCollector;
    use crate::graphics::renderer::event_loop::physics_coordinator::PhysicsCoordinator;
    use crate::graphics::renderer::event_loop::unified_render_state::{
        PendingWindowRequest, RenderStateData, UnifiedRenderState,
    };
    use crate::graphics::renderer::margins::Margins;
    use crate::graphics::renderer::state_layers::RendererConfig;
    use crate::objects2d::{Rect, Vec2D};
    use std::sync::{Arc, Mutex};

    fn dummy_viewport() -> ViewportTransform {
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        ViewportTransform::new(world, (1280.0, 720.0), Margins::symmetric(50.0, 50.0))
    }

    fn build_state_with_physics(physics: PhysicsCoordinator) -> UnifiedRenderState {
        let energy_data: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));
        UnifiedRenderState {
            input: InputHandler::new(),
            physics,
            render: RenderStateData {
                viewport: dummy_viewport(),
                energy_manager: EnergyWindowManager::new(energy_data),
                config: RendererConfig::default(),
                view_dirty: false,
                last_snapshot: None,
                cursor_world_pos: Vec2D::zero(),
                metrics: MetricsCollector::new(),
                active_tool: OperatorTool::None,
                tool_handler: None,
                default_operator_snapshot: OperatorSnapshot::default(),
                resize_pending: false,
                requested_window_size: None,
                pending_window_request: None,
            },
        }
    }

    fn build_test_state() -> UnifiedRenderState {
        build_state_with_physics(PhysicsCoordinator::new_disconnected())
    }

    fn build_test_state_recording() -> (UnifiedRenderState, Arc<Mutex<Vec<String>>>) {
        let (physics, log) = PhysicsCoordinator::new_recording();
        (build_state_with_physics(physics), log)
    }

    #[test]
    fn zoom_at_point_marks_view_dirty() {
        let mut state = build_test_state();

        ViewportOrchestrator::zoom_at_point(&mut state, &Vec2D::zero(), 2.0);

        assert!(state.render.view_dirty);
    }

    #[test]
    fn apply_pan_marks_view_dirty() {
        let mut state = build_test_state();
        state.input.start_pan([0.0, 0.0], Vec2D::zero());

        ViewportOrchestrator::apply_pan_from_input(&mut state, Vec2D::new(5.0, 3.0));

        assert!(state.render.view_dirty);
    }

    #[test]
    fn handle_resize_sets_pending_resume() {
        let mut state = build_test_state();

        ViewportOrchestrator::handle_resize(
            &mut state,
            1024.0,
            768.0,
            Margins::symmetric(50.0, 50.0),
        );

        assert!(state.render.view_dirty);
        assert!(state.render.resize_pending);
    }

    #[test]
    fn handle_resize_sends_resize_command_to_physics() {
        let (mut state, log) = build_test_state_recording();

        ViewportOrchestrator::handle_resize(
            &mut state,
            1024.0,
            768.0,
            Margins::symmetric(50.0, 50.0),
        );

        let commands = log.lock().expect("command log lock should succeed");
        assert!(
            commands.iter().any(|c| c.starts_with("World(Resize {")),
            "expected a Resize command to be forwarded to physics; got: {commands:?}"
        );
    }

    #[test]
    fn handle_resize_with_unchanged_size_leaves_physics_alone() {
        let (mut state, log) = build_test_state_recording();
        let (w, h) = state.render.viewport.window_size_px();
        let margins = state.render.viewport.margins();

        ViewportOrchestrator::handle_resize(&mut state, w, h, margins);

        let commands = log.lock().expect("command log lock should succeed");
        assert!(commands.is_empty(), "no Resize expected; got: {commands:?}");
    }

    #[test]
    fn os_answer_after_a_load_fits_the_view_but_keeps_the_world() {
        let (mut state, log) = build_test_state_recording();
        let loaded = Rect::new(-50.0, 50.0, 100.0, -100.0);
        state.render.viewport.set_world_bounds(loaded);
        state.render.pending_window_request = Some(PendingWindowRequest {
            size: (1100, 1100),
            at: std::time::Instant::now(),
        });

        // The OS grants another size than asked for, right away.
        ViewportOrchestrator::handle_resize(
            &mut state,
            900.0,
            700.0,
            Margins::symmetric(50.0, 50.0),
        );
        assert!(
            log.lock().unwrap().is_empty(),
            "the loaded world is not rescaled"
        );
        assert_eq!(state.render.viewport.window_size_px(), (900.0, 700.0));
        assert_eq!(state.render.viewport.world_bounds(), loaded);
        assert!(state.render.pending_window_request.is_none());
        assert!(state.render.view_dirty);

        // A later (user) resize refits the world to the window as before.
        ViewportOrchestrator::handle_resize(
            &mut state,
            1000.0,
            700.0,
            Margins::symmetric(50.0, 50.0),
        );
        assert!(
            log.lock()
                .unwrap()
                .iter()
                .any(|c| c.starts_with("World(Resize {"))
        );
    }

    #[test]
    fn an_overdue_window_request_does_not_decide_a_later_resize() {
        let (mut state, log) = build_test_state_recording();
        // Asked long ago; the window manager ignored it (maximised window).
        state.render.pending_window_request = Some(PendingWindowRequest {
            size: (1100, 1100),
            at: std::time::Instant::now() - std::time::Duration::from_secs(30),
        });

        ViewportOrchestrator::handle_resize(
            &mut state,
            900.0,
            700.0,
            Margins::symmetric(50.0, 50.0),
        );
        assert!(
            log.lock()
                .unwrap()
                .iter()
                .any(|c| c.starts_with("World(Resize {")),
            "the user's resize refits the world"
        );
        assert!(state.render.pending_window_request.is_none());
    }

    #[test]
    fn a_late_answer_of_the_requested_size_still_keeps_the_world() {
        let (mut state, log) = build_test_state_recording();
        state.render.pending_window_request = Some(PendingWindowRequest {
            size: (1100, 1100),
            at: std::time::Instant::now() - std::time::Duration::from_secs(30),
        });

        ViewportOrchestrator::handle_resize(
            &mut state,
            1100.0,
            1100.0,
            Margins::symmetric(50.0, 50.0),
        );
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn resume_after_resize_clears_pending_flag() {
        let mut state = build_test_state();
        state.render.resize_pending = true;

        ViewportOrchestrator::resume_after_resize_if_pending(&mut state);

        assert!(!state.render.resize_pending);
    }

    #[test]
    fn resume_after_resize_sends_resume_command_to_physics() {
        let (mut state, log) = build_test_state_recording();
        state.render.resize_pending = true;

        ViewportOrchestrator::resume_after_resize_if_pending(&mut state);

        let commands = log.lock().expect("command log lock should succeed");
        assert!(
            commands.iter().any(|c| c == "World(ResumeAfterResize)"),
            "expected a ResumeAfterResize command to be forwarded to physics; got: {commands:?}"
        );
    }

    #[test]
    fn update_cursor_world_pos_tracks_input_and_state() {
        let mut state = build_test_state();

        ViewportOrchestrator::update_cursor_world_pos(&mut state, &[100.0, 120.0]);

        assert_eq!(state.render.cursor_world_pos, state.input.cursor_pos());
        assert_eq!(state.input.cursor_pos_px(), [100.0, 120.0]);
    }
}
