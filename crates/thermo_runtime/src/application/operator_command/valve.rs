// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Valve operator commands.

use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;

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
    fn toggle_valve_fields_round_trip() {
        let cmd = ToggleValve { valve_id: 3 };
        assert_eq!(cmd.valve_id, 3);
    }
}
