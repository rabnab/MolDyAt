// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::valve_segment_tool::{ValveDrawState, ValveGrab, ValveGrip};
use thermo_physics::application::core::ValveSpanChange;
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
        grab: None,
        dragging: false,
    };
    if let ValveDrawState::Pressed {
        valve_id,
        press_pos,
        grab,
        dragging,
    } = state
    {
        assert_eq!(valve_id, 3);
        assert!(grab.is_none() && !dragging);
        assert!((press_pos.x - 1.5).abs() < 1e-6);
        assert!((press_pos.y + 2.0).abs() < 1e-6);
    } else {
        panic!("Expected Pressed state");
    }
}

/// A grab on a diagonal wall follows the cursor's projection onto it (#203).
#[test]
fn valve_grab_follows_the_projection_on_a_slanted_wall() {
    let grab = ValveGrab {
        grip: ValveGrip::Middle,
        chain_start: Vec2D::new(0.0, 0.0),
        chain_end: Vec2D::new(10.0, 10.0),
        t_start: 0.4,
        t_end: 0.5,
        t_press: 0.45,
    };
    // (6.5, 5.5) projects onto the wall at fraction 0.6: a shift of 0.15,
    // whatever the cursor's distance from the wall.
    let ValveSpanChange::Move { t_start } = grab.target(Vec2D::new(6.5, 5.5)).unwrap() else {
        panic!("a middle grab moves the valve");
    };
    assert!((t_start - 0.55).abs() < 1e-5, "{t_start}");
    let end = ValveGrab {
        grip: ValveGrip::End,
        ..grab
    };
    let ValveSpanChange::Resize { t_start, t_end } = end.target(Vec2D::new(6.5, 5.5)).unwrap()
    else {
        panic!("an end grab resizes the valve");
    };
    assert!(
        (t_start - 0.4).abs() < 1e-5 && (t_end - 0.65).abs() < 1e-5,
        "{t_start}..{t_end}"
    );
}
