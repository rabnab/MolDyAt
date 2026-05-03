use super::action_processor::process_action_common;
use super::input_handler::{InputAction, OperatorTool};
use super::operator_tool_handler::scroll_y_from_delta;
use super::unified_render_state::UnifiedRenderState;

/// Coordinates renderer-side input handling, tool flow, and action dispatch.
pub(super) struct InputOrchestrator;

impl InputOrchestrator {
    pub(super) fn process_action(
        state: &mut UnifiedRenderState,
        action: InputAction,
        window_target: Option<&winit::event_loop::ActiveEventLoop>,
        main_window: Option<&winit::window::Window>,
    ) -> bool {
        match action {
            InputAction::ResetView => {
                state.reset_view();
                true
            }
            InputAction::NextBallSize => {
                state.next_ball_size();
                if state.active_tool == OperatorTool::Ball {
                    state.view_dirty = true;
                }
                false
            }
            InputAction::ToggleEnergyWindow => {
                if let Some(target) = window_target {
                    state.toggle_energy_window(target, main_window);
                }
                false
            }
            InputAction::SelectOperatorTool(tool) => {
                if let Some(handler) = state.tool_handler.as_mut() {
                    handler.reset();
                }
                state.active_tool = tool;
                state.tool_handler = UnifiedRenderState::handler_for_tool(tool);
                state.config.draw_cursor = tool == OperatorTool::Ball;
                state.view_dirty = true;
                false
            }
            InputAction::AddBall { position } => {
                let operators = state
                    .last_snapshot
                    .as_deref()
                    .map(|s| &s.operators)
                    .unwrap_or(&state.default_operator_snapshot);
                if operators.is_near_any_operator(position) {
                    return false;
                }
                process_action_common(
                    InputAction::AddBall { position },
                    &state.physics,
                    &mut state.config,
                );
                false
            }
            other => {
                let affects_view = process_action_common(other, &state.physics, &mut state.config);
                if affects_view {
                    state.view_dirty = true;
                }
                affects_view
            }
        }
    }

    pub(super) fn handle_mouse_event(
        state: &mut UnifiedRenderState,
        event: &winit::event::WindowEvent,
    ) -> bool {
        match event {
            winit::event::WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => Self::handle_mouse_input_event(state, *button, *button_state),
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                Self::handle_cursor_moved_event(state, position.x as f32, position.y as f32)
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                Self::handle_mouse_wheel_event(state, *delta)
            }
            _ => false,
        }
    }

    pub(super) fn handle_mouse_input_event(
        state: &mut UnifiedRenderState,
        button: winit::event::MouseButton,
        element_state: winit::event::ElementState,
    ) -> bool {
        if element_state == winit::event::ElementState::Released {
            state.resume_after_resize_if_pending();
        }

        let world_pos = state.input.cursor_pos();
        let screen_pos = state.input.cursor_pos_px();
        let pan_offset = state.viewport.pan_offset();

        if button == winit::event::MouseButton::Left
            && element_state == winit::event::ElementState::Pressed
            && let Some(icon_idx) = state.sidebar_icon_at(screen_pos)
        {
            let tool = UnifiedRenderState::operator_tool_for_icon(icon_idx);
            let new_tool = if state.active_tool == tool {
                OperatorTool::None
            } else {
                tool
            };
            Self::process_action(state, InputAction::SelectOperatorTool(new_tool), None, None);
            return true;
        }

        if state.active_tool == OperatorTool::Ball
            && button == winit::event::MouseButton::Left
            && element_state == winit::event::ElementState::Released
        {
            return Self::process_action(
                state,
                InputAction::AddBall {
                    position: world_pos,
                },
                None,
                None,
            );
        }

        if let Some(handler) = &mut state.tool_handler {
            let ctx = UnifiedRenderState::make_tool_context_from(
                state.last_snapshot.as_deref(),
                &state.default_operator_snapshot,
                &state.viewport,
                state.cursor_world_pos,
            );
            let resp = handler.handle_mouse_button(button, element_state, world_pos, &ctx);
            if resp.consumed {
                if let Some(cmd) = resp.action {
                    Self::process_action(state, InputAction::OperatorAction(cmd), None, None);
                }
                if resp.view_dirty {
                    state.view_dirty = true;
                }
                return true;
            }
        }

        if let Some(action) = state.input.handle_mouse_button(
            button,
            element_state,
            screen_pos,
            world_pos,
            pan_offset,
        ) {
            return Self::process_action(state, action, None, None);
        }
        false
    }

    pub(super) fn handle_cursor_moved_event(
        state: &mut UnifiedRenderState,
        x: f32,
        y: f32,
    ) -> bool {
        state.update_cursor_world_pos(&[x, y]);

        if let Some(handler) = &mut state.tool_handler {
            let world_pos = state.input.cursor_pos();
            let ctx = UnifiedRenderState::make_tool_context_from(
                state.last_snapshot.as_deref(),
                &state.default_operator_snapshot,
                &state.viewport,
                state.cursor_world_pos,
            );
            let resp = handler.handle_cursor_move(world_pos, &ctx);
            if resp.consumed {
                if let Some(cmd) = resp.action {
                    Self::process_action(state, InputAction::OperatorAction(cmd), None, None);
                }
                if resp.view_dirty {
                    state.view_dirty = true;
                }
            }
        }

        if let Some(action) = state.input.handle_mouse_motion(x, y) {
            if let InputAction::PanUpdate { delta } = &action {
                state.apply_pan_from_input(*delta);
                return true;
            }
            return Self::process_action(state, action, None, None);
        }
        false
    }

    pub(super) fn handle_mouse_wheel_event(
        state: &mut UnifiedRenderState,
        delta: winit::event::MouseScrollDelta,
    ) -> bool {
        let cursor_pos = state.input.cursor_pos();

        if let Some(handler) = &mut state.tool_handler {
            let scroll_y = scroll_y_from_delta(delta);
            if scroll_y.abs() > f32::EPSILON {
                let ctrl_held = state
                    .input
                    .is_key_pressed(winit::keyboard::KeyCode::ControlLeft)
                    || state
                        .input
                        .is_key_pressed(winit::keyboard::KeyCode::ControlRight);
                let ctx = UnifiedRenderState::make_tool_context_from(
                    state.last_snapshot.as_deref(),
                    &state.default_operator_snapshot,
                    &state.viewport,
                    state.cursor_world_pos,
                );
                let resp = handler.handle_scroll(scroll_y, cursor_pos, ctrl_held, &ctx);
                if resp.consumed {
                    if let Some(cmd) = resp.action {
                        Self::process_action(state, InputAction::OperatorAction(cmd), None, None);
                    }
                    if resp.view_dirty {
                        state.view_dirty = true;
                    }
                    return true;
                }
            }
        }

        if let Some(action) = state.input.handle_mouse_wheel(delta, cursor_pos) {
            match &action {
                InputAction::ZoomIn { center } => {
                    state.zoom_at_point(center, 1.1);
                    Self::process_action(state, action, None, None);
                    true
                }
                InputAction::ZoomOut { center } => {
                    state.zoom_at_point(center, 1.0 / 1.1);
                    Self::process_action(state, action, None, None);
                    true
                }
                InputAction::ResetView => {
                    state.reset_view();
                    Self::process_action(state, action, None, None);
                    true
                }
                _ => Self::process_action(state, action, None, None),
            }
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InputOrchestrator;
    use crate::application::renderer_snapshot::OperatorSnapshot;
    use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
    use crate::graphics::renderer::ViewportTransform;
    use crate::graphics::renderer::event_loop::energy_window_manager::EnergyWindowManager;
    use crate::graphics::renderer::event_loop::input_handler::{
        InputAction, InputHandler, OperatorTool,
    };
    use crate::graphics::renderer::event_loop::metrics::MetricsCollector;
    use crate::graphics::renderer::event_loop::physics_coordinator::PhysicsCoordinator;
    use crate::graphics::renderer::event_loop::unified_render_state::UnifiedRenderState;
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
        }
    }

    #[test]
    fn select_ball_tool_enables_cursor_preview() {
        let mut state = build_test_state();

        let affects_view = InputOrchestrator::process_action(
            &mut state,
            InputAction::SelectOperatorTool(OperatorTool::Ball),
            None,
            None,
        );

        assert!(!affects_view);
        assert_eq!(state.active_tool, OperatorTool::Ball);
        assert!(state.config.draw_cursor);
        assert!(state.view_dirty);
    }

    #[test]
    fn next_ball_size_marks_view_dirty_when_ball_tool_active() {
        let mut state = build_test_state();
        state.active_tool = OperatorTool::Ball;

        let affects_view =
            InputOrchestrator::process_action(&mut state, InputAction::NextBallSize, None, None);

        assert!(!affects_view);
        assert_eq!(state.current_ball_size_idx, 3);
        assert!(state.view_dirty);
    }
}
