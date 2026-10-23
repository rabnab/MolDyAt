// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::command_registry::KeyInput;
use super::super::input_handler::{InputAction, InputHandler, KeyCode, key_symbol};
use winit::keyboard::Key;

/// The `=` key of a US keyboard.
fn us_equal() -> KeyInput {
    KeyInput::new(KeyCode::Equal, Some('='))
}

/// The `-` key of a US keyboard.
fn us_minus() -> KeyInput {
    KeyInput::new(KeyCode::Minus, Some('-'))
}

/// The `+` key of a German keyboard (US `]`).
fn de_plus() -> KeyInput {
    KeyInput::new(KeyCode::BracketRight, Some('+'))
}

/// The `-` key of a German keyboard (US `/`).
fn de_minus() -> KeyInput {
    KeyInput::new(KeyCode::Slash, Some('-'))
}

/// The numpad `+` / `-`: they type the same characters as the main row.
fn numpad(code: KeyCode, symbol: char) -> [KeyInput; 1] {
    [KeyInput::new(code, Some(symbol))]
}

/// The registry id of the command a key event produced.
fn id(action: Option<InputAction>) -> Option<&'static str> {
    action.map(|action| action.command().expect("a registry command").id)
}

#[test]
fn test_space_toggles_pause() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_down(super::us(KeyCode::Space), false);
    assert_eq!(id(action), Some("sim.toggle_pause"));
}

#[test]
fn test_lcontrol_no_action() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_down(super::us(KeyCode::ControlLeft), false);
    assert!(action.is_none());
}

#[test]
fn test_escape_clears_balls() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_down(super::us(KeyCode::Escape), false);
    assert_eq!(id(action), Some("balls.clear"));
}

#[test]
fn test_tab_toggles_energy_window() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::Tab));
    assert_eq!(id(action), Some("view.toggle_energy_window"));
}

#[test]
fn test_f2_toggles_rendering() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::F2));
    assert_eq!(id(action), Some("view.toggle_rendering"));
}

#[test]
fn test_p_cycles_plot() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::KeyP));
    assert_eq!(id(action), Some("view.cycle_plot"));
}

#[test]
fn test_f1_stops_all_balls() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::F1));
    assert_eq!(id(action), Some("balls.stop_all"));
}

#[test]
fn test_key0_resets_view() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::Digit0));
    assert_eq!(id(action), Some("view.reset"));
}

#[test]
fn test_r_randomizes_speeds() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::KeyR));
    assert_eq!(id(action), Some("balls.randomize_speeds"));
}

#[test]
fn test_s_toggles_stepwise() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::KeyS));
    assert_eq!(id(action), Some("sim.toggle_stepwise"));
}

#[test]
fn test_h_prints_help() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::KeyH));
    assert_eq!(id(action), Some("help.show"));
}

#[test]
fn test_d_toggles_real_data() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::KeyD));
    assert_eq!(id(action), Some("view.toggle_real_data"));
}

#[test]
fn test_g_toggles_gravity() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::KeyG));
    assert_eq!(id(action), Some("sim.cycle_gravity"));
}

#[test]
fn test_f3_toggles_cutoff_rings() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::F3));
    assert_eq!(id(action), Some("view.cycle_cutoff_rings"));
}

#[test]
fn test_t_cycles_thermo_display() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::KeyT));
    assert_eq!(id(action), Some("view.cycle_thermo_overlay"));
}

#[test]
fn test_f4_toggles_timing_display() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::F4));
    assert_eq!(id(action), Some("debug.toggle_timings"));
}

#[test]
fn test_b_toggles_ball_stats() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::KeyB));
    assert_eq!(id(action), Some("view.toggle_ball_stats"));
}

#[test]
fn test_v_starts_verbosity_input() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::KeyV));
    assert_eq!(id(action), Some("debug.ball_verbosity"));
}

#[test]
fn test_f5_adds_random_balls() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::F5));
    assert_eq!(id(action), Some("balls.add_random"));
}

#[test]
fn test_lcontrol_up_no_action() {
    let mut handler = InputHandler::new();
    let action = handler.handle_key_up(super::us(KeyCode::ControlLeft));
    assert!(action.is_none());
}

#[test]
fn test_ctrl_s_saves_on_key_press_and_release_is_consumed() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(super::us(KeyCode::ControlLeft), false);
    let action = handler.handle_key_down(super::us(KeyCode::KeyS), false);
    assert_eq!(id(action), Some("world.save"));
    assert!(handler.handle_key_up(super::us(KeyCode::KeyS)).is_none());
}

#[test]
fn test_releasing_ctrl_before_s_does_not_toggle_stepwise() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(super::us(KeyCode::ControlLeft), false);
    assert_eq!(
        id(handler.handle_key_down(super::us(KeyCode::KeyS), false)),
        Some("world.save")
    );
    handler.handle_key_up(super::us(KeyCode::ControlLeft));
    assert!(handler.handle_key_up(super::us(KeyCode::KeyS)).is_none());

    // A later plain S press still toggles stepwise mode.
    handler.handle_key_down(super::us(KeyCode::KeyS), false);
    let action = handler.handle_key_up(super::us(KeyCode::KeyS));
    assert_eq!(id(action), Some("sim.toggle_stepwise"));
}

#[test]
fn test_ctrl_chord_does_not_repeat() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(super::us(KeyCode::ControlLeft), false);
    assert!(
        handler
            .handle_key_down(super::us(KeyCode::KeyS), false)
            .is_some()
    );
    assert!(
        handler
            .handle_key_down(super::us(KeyCode::KeyS), true)
            .is_none()
    );
}

#[test]
fn test_ctrl_o_loads_world() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(super::us(KeyCode::ControlRight), false);
    let action = handler.handle_key_down(super::us(KeyCode::KeyO), false);
    assert_eq!(id(action), Some("world.load_latest"));
    assert!(handler.handle_key_up(super::us(KeyCode::KeyO)).is_none());
}

#[test]
fn test_o_without_ctrl_does_nothing() {
    let mut handler = InputHandler::new();
    assert!(
        handler
            .handle_key_down(super::us(KeyCode::KeyO), false)
            .is_none()
    );
    assert!(handler.handle_key_up(super::us(KeyCode::KeyO)).is_none());
}

#[test]
fn test_focus_loss_forgets_a_held_ctrl() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(super::us(KeyCode::ControlLeft), false);
    // Ctrl is released while another window has focus: no release arrives.
    handler.release_all_keys();
    handler.handle_key_down(super::us(KeyCode::KeyS), false);
    let action = handler.handle_key_up(super::us(KeyCode::KeyS));
    assert_eq!(id(action), Some("sim.toggle_stepwise"));
    assert!(
        handler
            .handle_key_down(super::us(KeyCode::KeyO), false)
            .is_none()
    );
}

#[test]
fn test_plus_and_minus_change_the_time_step() {
    let mut handler = InputHandler::new();
    let plus = [us_equal(), de_plus()].into_iter();
    for key in plus.chain(numpad(KeyCode::NumpadAdd, '+')) {
        assert!(handler.handle_key_down(key, false).is_none());
        assert_eq!(
            id(handler.handle_key_up(key)),
            Some("sim.double_time_step"),
            "{key:?}"
        );
    }
    let minus = [us_minus(), de_minus()].into_iter();
    for key in minus.chain(numpad(KeyCode::NumpadSubtract, '-')) {
        assert!(handler.handle_key_down(key, false).is_none());
        assert_eq!(
            id(handler.handle_key_up(key)),
            Some("sim.halve_time_step"),
            "{key:?}"
        );
    }
}

/// On a German keyboard the US `=` and `-` keys type `´` and `ß`: they must
/// not change the time step (#327).
#[test]
fn test_keys_in_the_us_plus_and_minus_places_do_nothing_on_other_layouts() {
    let mut handler = InputHandler::new();
    for key in [
        KeyInput::new(KeyCode::Equal, Some('´')),
        KeyInput::new(KeyCode::Minus, Some('ß')),
    ] {
        assert!(handler.handle_key_down(key, false).is_none());
        assert!(handler.handle_key_up(key).is_none(), "{key:?}");
    }
}

#[test]
fn test_shift_plus_on_a_us_keyboard_doubles_the_time_step() {
    // The character is taken without modifiers, so Shift + = is `=`; if a
    // platform reports `+`, that is bound too.
    for key in [us_equal(), KeyInput::new(KeyCode::Equal, Some('+'))] {
        let mut handler = InputHandler::new();
        handler.handle_key_down(super::us(KeyCode::ShiftLeft), false);
        assert!(handler.handle_key_down(key, false).is_none());
        assert_eq!(id(handler.handle_key_up(key)), Some("sim.double_time_step"));
    }
}

#[test]
fn test_release_runs_the_command_of_the_character_typed_on_press() {
    let mut handler = InputHandler::new();
    assert!(handler.handle_key_down(de_plus(), false).is_none());
    // A release without the character still belongs to the `+` press.
    assert_eq!(
        id(handler.handle_key_up(super::us(KeyCode::BracketRight))),
        Some("sim.double_time_step")
    );
    // Once released, the physical key alone has no binding.
    assert!(
        handler
            .handle_key_up(super::us(KeyCode::BracketRight))
            .is_none()
    );
}

/// Layouts where `+` needs Shift (Swiss: Shift + 1, JIS: Shift + `;`)
/// double the time step with the character typed with Shift (#354 review).
#[test]
fn test_plus_that_needs_shift_doubles_the_time_step() {
    for (code, unshifted) in [(KeyCode::Digit1, '1'), (KeyCode::Semicolon, ';')] {
        let key = KeyInput::new(code, Some(unshifted)).with_typed(Some('+'));
        let mut handler = InputHandler::new();
        handler.handle_key_down(super::us(KeyCode::ShiftLeft), false);
        assert!(handler.handle_key_down(key, false).is_none());
        assert_eq!(
            id(handler.handle_key_up(key)),
            Some("sim.double_time_step"),
            "{key:?}"
        );
    }
}

/// An auto-repeat keeps the characters of the first press: after a layout
/// switch while `+` is held, the release still doubles the time step.
#[test]
fn test_auto_repeat_keeps_the_character_of_the_press() {
    let mut handler = InputHandler::new();
    assert!(handler.handle_key_down(de_plus(), false).is_none());
    // The layout is now US: the same key repeats as `]`.
    let us_bracket = KeyInput::new(KeyCode::BracketRight, Some(']'));
    assert!(handler.handle_key_down(us_bracket, true).is_none());
    assert_eq!(
        id(handler.handle_key_up(us_bracket)),
        Some("sim.double_time_step")
    );
}

#[test]
fn test_key_symbol_takes_single_characters_only() {
    assert_eq!(key_symbol(&Key::Character("+".into())), Some('+'));
    assert_eq!(key_symbol(&Key::Character("ß".into())), Some('ß'));
    assert_eq!(key_symbol(&Key::Character("ab".into())), None);
    assert_eq!(
        key_symbol(&Key::Named(winit::keyboard::NamedKey::Space)),
        None
    );
}

#[test]
fn test_alt_plus_and_minus_change_the_wait_time() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(super::us(KeyCode::AltLeft), false);
    for key in [us_equal(), de_plus()] {
        handler.handle_key_down(key, false);
        assert_eq!(
            id(handler.handle_key_up(key)),
            Some("sim.decrease_wait_time"),
            "{key:?}"
        );
    }
    for key in [us_minus(), de_minus()] {
        handler.handle_key_down(key, false);
        assert_eq!(
            id(handler.handle_key_up(key)),
            Some("sim.increase_wait_time"),
            "{key:?}"
        );
    }
    handler.handle_key_down(super::us(KeyCode::NumpadSubtract), false);
    assert_eq!(
        id(handler.handle_key_up(super::us(KeyCode::NumpadSubtract))),
        Some("sim.increase_wait_time")
    );
    // Alt does not change keys without an Alt binding.
    assert_eq!(
        id(handler.handle_key_up(super::us(KeyCode::KeyG))),
        Some("sim.cycle_gravity")
    );
}

#[test]
fn test_numpad0_resets_view() {
    let mut handler = InputHandler::new();
    assert_eq!(
        id(handler.handle_key_up(super::us(KeyCode::Numpad0))),
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
        id(handler.handle_key_down(super::us(KeyCode::Space), true)),
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
    handler.handle_key_down(super::us(KeyCode::ControlLeft), false);
    assert_eq!(
        id(handler.handle_key_down(super::us(KeyCode::Space), false)),
        Some("sim.toggle_pause")
    );
    assert_eq!(
        id(handler.handle_key_up(super::us(KeyCode::KeyG))),
        Some("sim.cycle_gravity")
    );
}

#[test]
fn test_alt_released_before_plus_still_decreases_wait_time() {
    let mut handler = InputHandler::new();
    handler.handle_key_down(super::us(KeyCode::AltLeft), false);
    assert!(handler.handle_key_down(us_equal(), false).is_none());
    assert!(handler.handle_key_up(super::us(KeyCode::AltLeft)).is_none());
    assert_eq!(
        id(handler.handle_key_up(us_equal())),
        Some("sim.decrease_wait_time")
    );
    // The next plain = doubles the time step again.
    handler.handle_key_down(us_equal(), false);
    assert_eq!(
        id(handler.handle_key_up(us_equal())),
        Some("sim.double_time_step")
    );
}

#[test]
fn test_modifier_pressed_after_a_plain_key_does_not_make_a_chord() {
    for repeat_in_between in [false, true] {
        let mut handler = InputHandler::new();
        assert!(handler.handle_key_down(us_equal(), false).is_none());
        handler.handle_key_down(super::us(KeyCode::AltLeft), false);
        if repeat_in_between {
            assert!(handler.handle_key_down(us_equal(), true).is_none());
        }
        assert_eq!(
            id(handler.handle_key_up(us_equal())),
            Some("sim.double_time_step"),
            "repeat in between: {repeat_in_between}"
        );
    }
}

#[test]
fn test_ctrl_pressed_while_s_is_held_does_not_save() {
    let mut handler = InputHandler::new();
    assert!(
        handler
            .handle_key_down(super::us(KeyCode::KeyS), false)
            .is_none()
    );
    handler.handle_key_down(super::us(KeyCode::ControlLeft), false);
    assert!(
        handler
            .handle_key_down(super::us(KeyCode::KeyS), true)
            .is_none()
    );
    assert_eq!(
        id(handler.handle_key_up(super::us(KeyCode::KeyS))),
        Some("sim.toggle_stepwise")
    );
}
