// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::input_handler::{InputAction, InputHandler, KeyCode};

/// The registry id of the command a key event produced.
fn id(action: Option<InputAction>) -> Option<&'static str> {
    action.map(|action| action.command().expect("a registry command").id)
}

#[test]
fn test_space_toggles_pause() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_down(KeyCode::Space, false);
    assert_eq!(id(action), Some("sim.toggle_pause"));
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
    assert_eq!(id(action), Some("balls.clear"));
}

#[test]
fn test_tab_toggles_energy_window() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::Tab);
    assert_eq!(id(action), Some("view.toggle_energy_window"));
}

#[test]
fn test_f2_toggles_rendering() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::F2);
    assert_eq!(id(action), Some("view.toggle_rendering"));
}

#[test]
fn test_p_cycles_plot() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::KeyP);
    assert_eq!(id(action), Some("view.cycle_plot"));
}

#[test]
fn test_f1_stops_all_balls() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::F1);
    assert_eq!(id(action), Some("balls.stop_all"));
}

#[test]
fn test_key0_resets_view() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::Digit0);
    assert_eq!(id(action), Some("view.reset"));
}

#[test]
fn test_r_randomizes_speeds() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::KeyR);
    assert_eq!(id(action), Some("balls.randomize_speeds"));
}

#[test]
fn test_s_toggles_stepwise() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::KeyS);
    assert_eq!(id(action), Some("sim.toggle_stepwise"));
}

#[test]
fn test_h_prints_help() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::KeyH);
    assert_eq!(id(action), Some("help.show"));
}

#[test]
fn test_d_toggles_real_data() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::KeyD);
    assert_eq!(id(action), Some("view.toggle_real_data"));
}

#[test]
fn test_g_toggles_gravity() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::KeyG);
    assert_eq!(id(action), Some("sim.cycle_gravity"));
}

#[test]
fn test_f3_toggles_cutoff_rings() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::F3);
    assert_eq!(id(action), Some("view.cycle_cutoff_rings"));
}

#[test]
fn test_t_cycles_thermo_display() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::KeyT);
    assert_eq!(id(action), Some("view.cycle_thermo_overlay"));
}

#[test]
fn test_f4_toggles_timing_display() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::F4);
    assert_eq!(id(action), Some("debug.toggle_timings"));
}

#[test]
fn test_b_toggles_ball_stats() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::KeyB);
    assert_eq!(id(action), Some("view.toggle_ball_stats"));
}

#[test]
fn test_v_starts_verbosity_input() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::KeyV);
    assert_eq!(id(action), Some("debug.ball_verbosity"));
}

#[test]
fn test_f5_adds_random_balls() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::F5);
    assert_eq!(id(action), Some("balls.add_random"));
}

#[test]
fn test_lcontrol_up_no_action() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(KeyCode::ControlLeft);
    assert!(action.is_none());
}

#[test]
fn test_ctrl_s_saves_on_key_press_and_release_is_consumed() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(KeyCode::ControlLeft, false);
    let action = handler.handle_key_down(KeyCode::KeyS, false);
    assert_eq!(id(action), Some("world.save"));
    assert!(handler.handle_key_up(KeyCode::KeyS).is_none());
}

#[test]
fn test_releasing_ctrl_before_s_does_not_toggle_stepwise() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(KeyCode::ControlLeft, false);
    assert_eq!(
        id(handler.handle_key_down(KeyCode::KeyS, false)),
        Some("world.save")
    );
    handler.handle_key_up(KeyCode::ControlLeft);
    assert!(handler.handle_key_up(KeyCode::KeyS).is_none());

    // A later plain S press still toggles stepwise mode.
    handler.handle_key_down(KeyCode::KeyS, false);
    let action = handler.handle_key_up(KeyCode::KeyS);
    assert_eq!(id(action), Some("sim.toggle_stepwise"));
}

#[test]
fn test_ctrl_chord_does_not_repeat() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(KeyCode::ControlLeft, false);
    assert!(handler.handle_key_down(KeyCode::KeyS, false).is_some());
    assert!(handler.handle_key_down(KeyCode::KeyS, true).is_none());
}

#[test]
fn test_ctrl_o_loads_world() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(KeyCode::ControlRight, false);
    let action = handler.handle_key_down(KeyCode::KeyO, false);
    assert_eq!(id(action), Some("world.load_latest"));
    assert!(handler.handle_key_up(KeyCode::KeyO).is_none());
}

#[test]
fn test_o_without_ctrl_does_nothing() {
    let mut handler = InputHandler::new();
    assert!(handler.handle_key_down(KeyCode::KeyO, false).is_none());
    assert!(handler.handle_key_up(KeyCode::KeyO).is_none());
}

#[test]
fn test_focus_loss_forgets_a_held_ctrl() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(KeyCode::ControlLeft, false);
    // Ctrl is released while another window has focus: no release arrives.
    handler.release_all_keys();
    handler.handle_key_down(KeyCode::KeyS, false);
    let action = handler.handle_key_up(KeyCode::KeyS);
    assert_eq!(id(action), Some("sim.toggle_stepwise"));
    assert!(handler.handle_key_down(KeyCode::KeyO, false).is_none());
}

#[test]
fn test_plus_and_minus_change_the_time_step() {
    let mut handler = InputHandler::new();
    for key in [KeyCode::Equal, KeyCode::NumpadAdd] {
        assert!(handler.handle_key_down(key, false).is_none());
        assert_eq!(id(handler.handle_key_up(key)), Some("sim.double_time_step"));
    }
    for key in [KeyCode::Minus, KeyCode::NumpadSubtract] {
        assert!(handler.handle_key_down(key, false).is_none());
        assert_eq!(id(handler.handle_key_up(key)), Some("sim.halve_time_step"));
    }
}

#[test]
fn test_alt_plus_and_minus_change_the_wait_time() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(KeyCode::AltLeft, false);
    assert_eq!(
        id(handler.handle_key_up(KeyCode::Equal)),
        Some("sim.decrease_wait_time")
    );
    assert_eq!(
        id(handler.handle_key_up(KeyCode::NumpadSubtract)),
        Some("sim.increase_wait_time")
    );
    // Alt does not change keys without an Alt binding.
    assert_eq!(
        id(handler.handle_key_up(KeyCode::KeyG)),
        Some("sim.cycle_gravity")
    );
}

#[test]
fn test_numpad0_resets_view() {
    let mut handler = InputHandler::new();
    assert_eq!(
        id(handler.handle_key_up(KeyCode::Numpad0)),
        Some("view.reset")
    );
}

#[test]
fn test_delete_and_backspace_delete_hovered_once() {
    let mut handler = InputHandler::new();
    for key in [KeyCode::Delete, KeyCode::Backspace] {
        assert_eq!(
            id(handler.handle_key_down(key, false)),
            Some("tools.delete_hovered")
        );
        assert!(handler.handle_key_down(key, true).is_none());
        assert!(handler.handle_key_up(key).is_none());
    }
}

#[test]
fn test_space_repeats_while_held() {
    let mut handler = InputHandler::new();
    assert_eq!(
        id(handler.handle_key_down(KeyCode::Space, true)),
        Some("sim.toggle_pause")
    );
}

#[test]
fn test_shift_enables_random_ball_speed() {
    let mut handler = InputHandler::new();
    for key in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
        assert_eq!(
            id(handler.handle_key_down(key, false)),
            Some("balls.enable_random_speed")
        );
    }
}

#[test]
fn test_plain_keys_fire_while_ctrl_is_held() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(KeyCode::ControlLeft, false);
    assert_eq!(
        id(handler.handle_key_down(KeyCode::Space, false)),
        Some("sim.toggle_pause")
    );
    assert_eq!(
        id(handler.handle_key_up(KeyCode::KeyG)),
        Some("sim.cycle_gravity")
    );
}
