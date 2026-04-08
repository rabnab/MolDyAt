use super::super::valve_segment_tool::ValveDrawState;

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
fn valve_moving_holds_fields() {
    let state = ValveDrawState::Moving {
        valve_id: 3,
        line_id: 1,
        t_half_width: 0.15,
    };
    if let ValveDrawState::Moving {
        valve_id,
        line_id,
        t_half_width,
    } = state
    {
        assert_eq!(valve_id, 3);
        assert_eq!(line_id, 1);
        assert!((t_half_width - 0.15).abs() < 1e-6);
    } else {
        panic!("Expected Moving state");
    }
}
