// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::valve_segment_tool::ValveDrawState;
use thermo_physics::objects2d::Vec2D;

#[test]
fn valve_draw_state_defaults_to_idle() {
    let state = ValveDrawState::default();
    assert!(matches!(state, ValveDrawState::Idle));
}

#[test]
fn valve_drawing_holds_fields() {
    let state = ValveDrawState::Drawing {
        line_id: 7,
        t_start: 0.25,
    };
    if let ValveDrawState::Drawing { line_id, t_start } = state {
        assert_eq!(line_id, 7);
        assert!((t_start - 0.25).abs() < 1e-6);
    } else {
        panic!("Expected Drawing state");
    }
}

#[test]
fn valve_pressed_holds_fields() {
    let state = ValveDrawState::Pressed {
        valve_id: 3,
        press_pos: Vec2D::new(1.5, -2.0),
    };
    if let ValveDrawState::Pressed {
        valve_id,
        press_pos,
    } = state
    {
        assert_eq!(valve_id, 3);
        assert!((press_pos.x - 1.5).abs() < 1e-6);
        assert!((press_pos.y + 2.0).abs() < 1e-6);
    } else {
        panic!("Expected Pressed state");
    }
}
