use super::unified_render_state::UnifiedRenderState;

/// Coordinates per-frame snapshot handoff and renderer bookkeeping.
pub(super) struct FrameOrchestrator;

impl FrameOrchestrator {
    /// Pull the latest physics snapshot into render state and update
    /// snapshot-derived views such as the energy plot.
    pub(super) fn begin_frame(state: &mut UnifiedRenderState) {
        if let Some(snapshot) = state.physics.get_latest_snapshot() {
            state.render.last_snapshot = Some(snapshot);
            Self::update_energy_from_snapshot(state);
        }
    }

    /// Start render metrics collection for the current frame.
    pub(super) fn begin_render(state: &mut UnifiedRenderState) {
        state.render.metrics.begin_render();
    }

    /// Determine whether the backend should draw a frame.
    pub(super) fn should_render(state: &UnifiedRenderState) -> bool {
        state.render.config.render_drawing
            && (state.physics.is_new_step_available() || state.render.view_dirty)
    }

    /// Finalize frame bookkeeping after drawing completes.
    pub(super) fn end_frame(state: &mut UnifiedRenderState) {
        state.render.view_dirty = false;
        state.render.metrics.end_frame();
        if let Some(snapshot) = &state.render.last_snapshot {
            state.physics.mark_step_displayed(snapshot.simulation_step);
        }
    }

    fn update_energy_from_snapshot(state: &mut UnifiedRenderState) {
        if let Some(snapshot) = &state.render.last_snapshot {
            let frame_metrics = state.render.metrics.current().clone();
            state.render.energy_manager.update_from_snapshot(
                snapshot,
                state.render.metrics.average_fps(),
                state.physics.steps_per_frame(),
                &frame_metrics,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FrameOrchestrator;
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
    fn should_render_uses_view_dirty_when_rendering_enabled() {
        let mut state = build_test_state();
        state.render.view_dirty = true;

        assert!(FrameOrchestrator::should_render(&state));
    }

    #[test]
    fn should_render_respects_rendering_toggle() {
        let mut state = build_test_state();
        state.render.view_dirty = true;
        state.render.config.render_drawing = false;

        assert!(!FrameOrchestrator::should_render(&state));
    }

    #[test]
    fn end_frame_clears_view_dirty() {
        let mut state = build_test_state();
        state.render.view_dirty = true;

        FrameOrchestrator::end_frame(&mut state);

        assert!(!state.render.view_dirty);
    }
}
