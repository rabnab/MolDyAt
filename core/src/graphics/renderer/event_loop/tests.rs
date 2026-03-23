#[cfg(test)]
mod tests {
    use super::super::input_handler::{InputAction, InputHandler, KeyCode};
    use crate::objects2d::Vec2D;

    #[test]
    fn test_key_registration() {
        let mut handler = InputHandler::new();
        assert!(!handler.is_key_pressed(KeyCode::Space));

        handler.register_key_pressed(KeyCode::Space);
        assert!(handler.is_key_pressed(KeyCode::Space));

        handler.register_key_released(KeyCode::Space);
        assert!(!handler.is_key_pressed(KeyCode::Space));
    }

    #[test]
    fn test_alt_modifier_detection() {
        let mut handler = InputHandler::new();
        assert!(!handler.is_alt_modifier_pressed());

        handler.register_key_pressed(KeyCode::AltLeft);
        assert!(handler.is_alt_modifier_pressed());

        handler.register_key_released(KeyCode::AltLeft);
        assert!(!handler.is_alt_modifier_pressed());

        handler.register_key_pressed(KeyCode::AltRight);
        assert!(handler.is_alt_modifier_pressed());

        handler.register_key_released(KeyCode::AltRight);
        assert!(!handler.is_alt_modifier_pressed());
    }

    #[test]
    fn test_pan_state() {
        let mut handler = InputHandler::new();
        assert!(!handler.is_panning());

        handler.start_pan([100.0, 200.0], Vec2D::new(10.0, 20.0));
        assert!(handler.is_panning());
        assert_eq!(handler.pan_start_pos_px(), [100.0, 200.0]);
        assert_eq!(handler.pan_start_offset(), Vec2D::new(10.0, 20.0));

        handler.end_pan();
        assert!(!handler.is_panning());
    }

    #[test]
    fn test_space_toggles_pause() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::Space, false);
        assert!(matches!(action, Some(InputAction::TogglePause)));
    }

    #[test]
    fn test_lcontrol_enables_cursor() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::ControlLeft, false);
        assert!(matches!(
            action,
            Some(InputAction::EnableCursor { repeated: false })
        ));
    }

    #[test]
    fn test_escape_clears_balls() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::Escape, false);
        assert!(matches!(action, Some(InputAction::ClearBalls)));
    }

    #[test]
    fn test_tab_toggles_energy_window() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::Tab);
        assert!(matches!(action, Some(InputAction::ToggleEnergyWindow)));
    }

    #[test]
    fn test_f2_toggles_rendering() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::F2);
        assert!(matches!(action, Some(InputAction::ToggleRendering)));
    }

    #[test]
    fn test_p_cycles_plot() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::KeyP);
        assert!(matches!(action, Some(InputAction::CyclePlotContent)));
    }

    #[test]
    fn test_f1_stops_all_balls() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::F1);
        assert!(matches!(action, Some(InputAction::StopAllBalls)));
    }

    #[test]
    fn test_key0_resets_view() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::Digit0);
        assert!(matches!(action, Some(InputAction::ResetView)));
    }

    #[test]
    fn test_r_randomizes_speeds() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::KeyR);
        assert!(matches!(action, Some(InputAction::RandomizeSpeeds)));
    }

    #[test]
    fn test_s_toggles_stepwise() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::KeyS);
        assert!(matches!(action, Some(InputAction::ToggleStepwise)));
    }

    #[test]
    fn test_h_prints_help() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::KeyH);
        assert!(matches!(action, Some(InputAction::PrintHelp)));
    }

    #[test]
    fn test_d_toggles_real_data() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::KeyD);
        assert!(matches!(action, Some(InputAction::ToggleRealData)));
    }

    #[test]
    fn test_f3_toggles_cutoff_rings() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::F3);
        assert!(matches!(action, Some(InputAction::ToggleCutoffRings)));
    }

    #[test]
    fn test_t_cycles_thermo_display() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::KeyT);
        assert!(matches!(action, Some(InputAction::CycleThermoDisplay)));
    }

    #[test]
    fn test_f4_toggles_timing_display() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::F4);
        assert!(matches!(action, Some(InputAction::ToggleTimingDisplay)));
    }

    #[test]
    fn test_b_toggles_ball_stats() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::KeyB);
        assert!(matches!(action, Some(InputAction::ToggleBallStats)));
    }

    #[test]
    fn test_v_starts_verbosity_input() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::KeyV);
        assert!(matches!(action, Some(InputAction::StartVerbosityInput)));
    }

    #[test]
    fn test_f5_adds_random_balls() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::F5);
        assert!(matches!(
            action,
            Some(InputAction::AddRandomBalls { count: 100 })
        ));
    }

    #[test]
    fn test_lcontrol_up_adds_ball() {
        let mut handler = InputHandler::new();
        handler.set_cursor_pos(Vec2D::new(5.0, 10.0), [100.0, 200.0]);
        let action = handler.handle_key_up(KeyCode::ControlLeft);
        assert!(
            matches!(action, Some(InputAction::AddBall { position }) if position == Vec2D::new(5.0, 10.0))
        );
    }

    #[test]
    fn test_cursor_position_tracking() {
        let mut handler = InputHandler::new();
        let world_pos = Vec2D::new(5.0, 10.0);
        let screen_pos = [100.0, 200.0];

        handler.set_cursor_pos(world_pos, screen_pos);

        assert_eq!(handler.cursor_pos(), world_pos);
        assert_eq!(handler.cursor_pos_px(), screen_pos);
    }

    #[test]
    fn test_right_button_cycles_ball_size() {
        let mut handler = InputHandler::new();
        let action = handler.handle_mouse_button(
            winit::event::MouseButton::Right,
            winit::event::ElementState::Pressed,
            [100.0, 200.0],
            Vec2D::new(5.0, 10.0),
            Vec2D::zero(),
        );
        assert!(matches!(action, Some(InputAction::NextBallSize)));
    }

    #[test]
    fn test_middle_button_starts_pan() {
        let mut handler = InputHandler::new();
        let action = handler.handle_mouse_button(
            winit::event::MouseButton::Middle,
            winit::event::ElementState::Pressed,
            [100.0, 200.0],
            Vec2D::new(5.0, 10.0),
            Vec2D::zero(),
        );
        assert!(
            matches!(action, Some(InputAction::PanStart { pos_px }) if pos_px == [100.0, 200.0])
        );
        assert!(handler.is_panning());
    }

    #[test]
    fn test_middle_button_ends_pan() {
        let mut handler = InputHandler::new();
        handler.start_pan([100.0, 200.0], Vec2D::zero());
        let action = handler.handle_mouse_button(
            winit::event::MouseButton::Middle,
            winit::event::ElementState::Released,
            [150.0, 250.0],
            Vec2D::new(7.0, 12.0),
            Vec2D::zero(),
        );
        assert!(matches!(action, Some(InputAction::PanEnd)));
        assert!(!handler.is_panning());
    }
}
