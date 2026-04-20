use super::super::input_handler::{InputAction, InputHandler, KeyCode};

#[test]
fn test_space_toggles_pause() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_down(KeyCode::Space, false);
    assert!(matches!(action, Some(InputAction::TogglePause)));
}

#[test]
fn test_lcontrol_no_action() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_down(KeyCode::ControlLeft, false);
    assert!(action.is_none());
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
fn test_lcontrol_up_no_action() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::ControlLeft);
    assert!(action.is_none());
}
