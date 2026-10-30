// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Valve operator commands.

use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;
use thermo_physics::application::core::ValveSpanChange;

/// Add a new valve on an existing wall piece.
///
/// `t_start`/`t_end` are fractions along the piece `line_id` itself; the
/// piece is split around the valve (see `SimulationWorld::add_valve_on_piece`).
#[derive(Debug)]
pub struct AddValveSegment {
    pub line_id: usize,
    pub t_start: f32,
    pub t_end: f32,
    pub is_closed: bool,
}

impl OperatorCommand for AddValveSegment {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.add_valve_segment(self.line_id, self.t_start, self.t_end, self.is_closed);
    }
}

/// Toggle (open ↔ closed) an existing valve.
#[derive(Debug)]
pub struct ToggleValve {
    pub valve_id: usize,
}

impl OperatorCommand for ToggleValve {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.toggle_valve(self.valve_id);
    }
}

/// Move or resize a valve along its wall (#203) as `change` says, in
/// fractions of its wall (chain), clamped so it stays between its
/// neighbouring pieces (see `MainContent::set_valve_span`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetValveSpan {
    pub valve_id: usize,
    pub change: ValveSpanChange,
}

impl OperatorCommand for SetValveSpan {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.set_valve_span(self.valve_id, self.change);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_valve_segment_fields_round_trip() {
        let cmd = AddValveSegment {
            line_id: 1,
            t_start: 0.1,
            t_end: 0.9,
            is_closed: true,
        };
        assert_eq!(cmd.line_id, 1);
        assert!((cmd.t_start - 0.1).abs() < f32::EPSILON);
        assert!((cmd.t_end - 0.9).abs() < f32::EPSILON);
        assert!(cmd.is_closed);
    }

    #[test]
    fn set_valve_span_moves_the_valve() {
        let mut content = thermo_physics::test_support::new_main_content(
            crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0),
        )
        .unwrap();
        content.add_line_segment(
            crate::objects2d::Vec2D::new(-10.0, 0.0),
            crate::objects2d::Vec2D::new(10.0, 0.0),
        );
        let valve_id = content.add_valve_segment(0, 0.4, 0.6, true).unwrap();
        Box::new(SetValveSpan {
            valve_id,
            change: ValveSpanChange::Move { t_start: 0.1 },
        })
        .execute(&mut content);
        use crate::application::CreateSnapshot;
        let span = content.create_snapshot().operators.valve_spans[0];
        assert!((span.p0.x + 8.0).abs() < 1e-4, "{span:?}");
        assert!((span.p1.x + 4.0).abs() < 1e-4, "{span:?}");
    }

    #[test]
    fn toggle_valve_fields_round_trip() {
        let cmd = ToggleValve { valve_id: 3 };
        assert_eq!(cmd.valve_id, 3);
    }
}
