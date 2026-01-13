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
    fn test_alt_key_registration() {
        let mut handler = InputHandler::new();
        assert!(!handler.is_alt_key_pressed(KeyCode::Minus));

        handler.register_alt_key_pressed(KeyCode::Minus);
        assert!(handler.is_alt_key_pressed(KeyCode::Minus));

        handler.register_alt_key_released(KeyCode::Minus);
        assert!(!handler.is_alt_key_pressed(KeyCode::Minus));
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
    fn test_s_triggers_single_step() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::S, false);
        assert!(matches!(action, Some(InputAction::SingleStep)));
    }

    #[test]
    fn test_s_repeated_returns_none() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::S, true);
        assert!(action.is_none());
    }

    #[test]
    fn test_escape_quits() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::Escape, false);
        assert!(matches!(action, Some(InputAction::Quit)));
    }

    #[test]
    fn test_f1_cycles_plot() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::F1, false);
        assert!(matches!(action, Some(InputAction::CyclePlotContent)));
    }

    #[test]
    fn test_f2_toggles_rendering() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::F2, false);
        assert!(matches!(action, Some(InputAction::ToggleRendering)));
    }

    #[test]
    fn test_f3_toggles_energy_window() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::F3, false);
        assert!(matches!(action, Some(InputAction::ToggleEnergyWindow)));
    }

    #[test]
    fn test_home_resets_view() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::Home, false);
        assert!(matches!(action, Some(InputAction::ResetView)));
    }

    #[test]
    fn test_r_resets() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_down(KeyCode::R, false);
        assert!(matches!(action, Some(InputAction::Reset)));
    }

    #[test]
    fn test_a_key_up_disables_cursor() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_up(KeyCode::A);
        assert!(matches!(action, Some(InputAction::DisableCursor)));
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
    fn test_default_ball_size_is_2() {
        let handler = InputHandler::new();
        assert_eq!(handler.get_selected_ball_size(), 2);
    }

    #[test]
    fn test_number_key_selects_ball_size() {
        let mut handler = InputHandler::new();

        // Test each number key
        handler.register_key_pressed(KeyCode::Key1);
        assert_eq!(handler.get_selected_ball_size(), 0);
        handler.register_key_released(KeyCode::Key1);

        handler.register_key_pressed(KeyCode::Key2);
        assert_eq!(handler.get_selected_ball_size(), 1);
        handler.register_key_released(KeyCode::Key2);

        handler.register_key_pressed(KeyCode::Key3);
        assert_eq!(handler.get_selected_ball_size(), 2);
        handler.register_key_released(KeyCode::Key3);

        handler.register_key_pressed(KeyCode::Key4);
        assert_eq!(handler.get_selected_ball_size(), 3);
        handler.register_key_released(KeyCode::Key4);

        handler.register_key_pressed(KeyCode::Key5);
        assert_eq!(handler.get_selected_ball_size(), 4);
    }
}
