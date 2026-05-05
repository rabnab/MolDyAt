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
                if state.is_active_tool(OperatorTool::Ball) {
                    state.mark_view_dirty();
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
                state.select_operator_tool(tool);
                false
            }
            InputAction::AddBall { position } => {
                if !state.can_add_ball_at(position) {
                    return false;
                }
                process_action_common(
                    InputAction::AddBall { position },
                    &state.physics,
                    &mut state.render.config,
                );
                false
            }
            other => {
                let affects_view =
                    process_action_common(other, &state.physics, &mut state.render.config);
                if affects_view {
                    state.mark_view_dirty();
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
        let pan_offset = state.viewport().pan_offset();

        if button == winit::event::MouseButton::Left
            && element_state == winit::event::ElementState::Pressed
            && let Some(icon_idx) = state.sidebar_icon_at(screen_pos)
        {
            let tool = UnifiedRenderState::operator_tool_for_icon(icon_idx);
            let new_tool = if state.is_active_tool(tool) {
                OperatorTool::None
            } else {
                tool
            };
            Self::process_action(state, InputAction::SelectOperatorTool(new_tool), None, None);
            return true;
        }

        if state.is_active_tool(OperatorTool::Ball)
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

        if let Some(resp) = state.with_tool_handler(|handler, ctx| {
            handler.handle_mouse_button(button, element_state, world_pos, &ctx)
        }) && resp.consumed
        {
            if let Some(cmd) = resp.action {
                Self::process_action(state, InputAction::OperatorAction(cmd), None, None);
            }
            if resp.view_dirty {
                state.mark_view_dirty();
            }
            return true;
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
        let world_pos = state.input.cursor_pos();

        if let Some(resp) =
            state.with_tool_handler(|handler, ctx| handler.handle_cursor_move(world_pos, &ctx))
            && resp.consumed
        {
            if let Some(cmd) = resp.action {
                Self::process_action(state, InputAction::OperatorAction(cmd), None, None);
            }
            if resp.view_dirty {
                state.mark_view_dirty();
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
        let ctrl_held = state
            .input
            .is_key_pressed(winit::keyboard::KeyCode::ControlLeft)
            || state
                .input
                .is_key_pressed(winit::keyboard::KeyCode::ControlRight);

        if let Some(resp) = state
            .with_tool_handler(|handler, ctx| {
                let scroll_y = scroll_y_from_delta(delta);
                if scroll_y.abs() <= f32::EPSILON {
                    return None;
                }
                Some(handler.handle_scroll(scroll_y, cursor_pos, ctrl_held, &ctx))
            })
            .flatten()
            && resp.consumed
        {
            if let Some(cmd) = resp.action {
                Self::process_action(state, InputAction::OperatorAction(cmd), None, None);
            }
            if resp.view_dirty {
                state.mark_view_dirty();
            }
            return true;
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
    use crate::application::renderer_snapshot::{OperatorSnapshot, RendererSnapshot};
    use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
    use crate::graphics::renderer::ViewportTransform;
    use crate::graphics::renderer::event_loop::energy_window_manager::EnergyWindowManager;
    use crate::graphics::renderer::event_loop::input_handler::{
        InputAction, InputHandler, OperatorTool,
    };
    use crate::graphics::renderer::event_loop::metrics::MetricsCollector;
    use crate::graphics::renderer::event_loop::physics_coordinator::PhysicsCoordinator;
    use crate::graphics::renderer::event_loop::unified_render_state::{
        RenderStateData, UnifiedRenderState,
    };
    use crate::graphics::renderer::margins::Margins;
    use crate::graphics::renderer::state_layers::RendererConfig;
    use crate::graphics::sidebar::{icon_display_size_for_margin, icon_spacing_for_margin};
    use crate::objects2d::{OperatorState, Rect, SimSourceSphere, Vec2D};
    use std::sync::{Arc, Mutex};
    use winit::event::{DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent};

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
                current_ball_size_idx: 2,
                metrics: MetricsCollector::new(),
                active_tool: OperatorTool::None,
                tool_handler: None,
                default_operator_snapshot: OperatorSnapshot::default(),
                resize_pending: false,
            },
        }
    }

    fn build_test_state() -> UnifiedRenderState {
        build_state_with_physics(PhysicsCoordinator::new_disconnected())
    }

    fn first_sidebar_icon_center(state: &UnifiedRenderState) -> [f32; 2] {
        let margins = state.viewport().margins();
        let icon_display_size = icon_display_size_for_margin(margins.left);
        let icon_spacing = icon_spacing_for_margin(margins.left);
        let pad_x = (margins.left - icon_display_size) * 0.5;
        let first_icon_top = margins.top + icon_spacing;
        [
            pad_x + icon_display_size * 0.5,
            first_icon_top + icon_display_size * 0.5,
        ]
    }

    fn dummy_device_id() -> DeviceId {
        // Test-only synthetic device id for constructing winit window events.
        DeviceId::dummy()
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
        assert_eq!(state.render.active_tool, OperatorTool::Ball);
        assert!(state.render.config.draw_cursor);
        assert!(state.render.view_dirty);
    }

    #[test]
    fn next_ball_size_marks_view_dirty_when_ball_tool_active() {
        let mut state = build_test_state();
        state.render.active_tool = OperatorTool::Ball;

        let affects_view =
            InputOrchestrator::process_action(&mut state, InputAction::NextBallSize, None, None);

        assert!(!affects_view);
        assert_eq!(state.render.current_ball_size_idx, 3);
        assert!(state.render.view_dirty);
    }

    #[test]
    fn handle_mouse_event_dispatches_sidebar_selection() {
        let mut state = build_test_state();
        let screen_pos = first_sidebar_icon_center(&state);
        state.input.set_cursor_pos(Vec2D::zero(), screen_pos);

        let event = WindowEvent::MouseInput {
            device_id: dummy_device_id(),
            state: ElementState::Pressed,
            button: MouseButton::Left,
        };

        let affects_view = InputOrchestrator::handle_mouse_event(&mut state, &event);

        assert!(affects_view);
        assert_eq!(state.render.active_tool, OperatorTool::Ball);
    }

    #[test]
    fn handle_mouse_input_event_forwards_to_active_tool_handler() {
        let mut state = build_test_state();
        state.select_operator_tool(OperatorTool::LineSegment);
        state.render.view_dirty = false;
        state
            .input
            .set_cursor_pos(Vec2D::new(3.0, 4.0), [200.0, 200.0]);

        let affects_view = InputOrchestrator::handle_mouse_input_event(
            &mut state,
            MouseButton::Left,
            ElementState::Pressed,
        );

        assert!(affects_view);
        assert!(state.render.view_dirty);
        assert_eq!(state.render.active_tool, OperatorTool::LineSegment);
    }

    #[test]
    fn handle_cursor_moved_event_applies_pan_fallthrough() {
        let mut state = build_test_state();
        state.input.start_pan([100.0, 200.0], Vec2D::zero());

        let affects_view = InputOrchestrator::handle_cursor_moved_event(&mut state, 150.0, 260.0);

        assert!(affects_view);
        assert!(state.render.view_dirty);
        assert!(state.input.is_panning());
    }

    #[test]
    fn handle_mouse_wheel_event_applies_zoom_fallthrough() {
        let mut state = build_test_state();
        state.input.set_cursor_pos(Vec2D::zero(), [120.0, 140.0]);
        let initial_zoom = state.viewport().zoom();

        let affects_view = InputOrchestrator::handle_mouse_wheel_event(
            &mut state,
            MouseScrollDelta::LineDelta(0.0, 1.0),
        );

        assert!(affects_view);
        assert!(state.viewport().zoom() > initial_zoom);
    }

    #[test]
    fn add_ball_blocked_when_position_is_near_an_operator() {
        let sphere_center = Vec2D::new(5.0, 5.0);

        // Build a snapshot containing a source sphere at sphere_center.
        let mut snapshot = RendererSnapshot::new();
        snapshot.operators.source_spheres.push(SimSourceSphere {
            id: 1,
            center: sphere_center,
            state: OperatorState::On,
            emit_period: 60,
            ball_size_idx: 2,
            emission_speed: 1.0,
        });

        // Use a recording coordinator so we can assert no AddBall command is sent.
        let (physics, log) = PhysicsCoordinator::new_recording();
        let mut state = build_state_with_physics(physics);
        state.render.active_tool = OperatorTool::Ball;
        state.render.last_snapshot = Some(std::sync::Arc::new(snapshot));

        // Attempt to place a ball exactly on top of the operator.
        let result = InputOrchestrator::process_action(
            &mut state,
            InputAction::AddBall {
                position: sphere_center,
            },
            None,
            None,
        );

        assert!(!result, "blocked ball placement should return false");
        let commands = log.lock().expect("command log lock should succeed");
        assert!(
            commands.iter().all(|c| !c.starts_with("AddBall")),
            "no AddBall command should be sent when placement is blocked; got: {commands:?}"
        );
    }
}
