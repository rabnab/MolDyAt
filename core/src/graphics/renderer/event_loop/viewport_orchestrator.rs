use super::unified_render_state::UnifiedRenderState;
use crate::application::DomainCommand;
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::margins::Margins;
use crate::objects2d::{Rect, Vec2D};

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
        state.render.viewport.set_margins(margin_px);
        let world_rect = ViewportTransform::compute_world_bounds_from_window_size(
            new_width as u32,
            new_height as u32,
            margin_px,
        );
        Self::handle_resize_with_world_rect(state, new_width, new_height, world_rect);
    }

    pub(super) fn handle_resize_with_world_rect(
        state: &mut UnifiedRenderState,
        new_width: f32,
        new_height: f32,
        world_rect: Rect,
    ) {
        state
            .physics
            .send_domain_command(DomainCommand::Resize { world_rect });
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
            state
                .physics
                .send_domain_command(DomainCommand::ResumeAfterResize);
        }
    }

    pub(super) fn apply_pan_from_input(state: &mut UnifiedRenderState, delta: Vec2D) {
        let start_offset = state.input.pan_start_offset();
        let zoom = state.render.viewport.zoom();
        let new_pan = ViewportTransform::apply_pan_delta(delta, start_offset, zoom);
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
        RenderStateData, UnifiedRenderState,
    };
    use crate::graphics::renderer::margins::Margins;
    use crate::graphics::renderer::state_layers::RendererConfig;
    use crate::objects2d::{Rect, Vec2D};
    use std::sync::{Arc, Mutex};

    fn dummy_viewport() -> ViewportTransform {
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        ViewportTransform::new(world, (1280.0, 720.0), Margins::symmetric(50.0, 50.0))
    }

    fn build_test_state() -> UnifiedRenderState {
        let energy_data: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));
        UnifiedRenderState {
            input: InputHandler::new(),
            physics: PhysicsCoordinator::new_disconnected(),
            render: RenderStateData {
                viewport: dummy_viewport(),
                energy_manager: EnergyWindowManager::new(energy_data),
                config: RendererConfig::default(),
                view_dirty: false,
                last_snapshot: None,
                cursor_world_pos: Vec2D::zero(),
                current_ball_size_idx: 2,
                metrics: MetricsCollector::new(),
                active_tool: OperatorTool::None,
                tool_handler: None,
                default_operator_snapshot: OperatorSnapshot::default(),
                resize_pending: false,
            },
        }
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
    fn update_cursor_world_pos_tracks_input_and_state() {
        let mut state = build_test_state();

        ViewportOrchestrator::update_cursor_world_pos(&mut state, &[100.0, 120.0]);

        assert_eq!(state.render.cursor_world_pos, state.input.cursor_pos());
        assert_eq!(state.input.cursor_pos_px(), [100.0, 120.0]);
    }
}
