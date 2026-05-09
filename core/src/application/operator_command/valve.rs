//! Valve operator commands.

use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;

/// Add a new valve segment on an existing line segment.
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

/// Reposition a valve segment along its parent line.
#[derive(Debug)]
pub struct MoveValveSegment {
    pub valve_id: usize,
    pub t_start: f32,
    pub t_end: f32,
}

impl OperatorCommand for MoveValveSegment {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.move_valve_segment(self.valve_id, self.t_start, self.t_end);
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

    #[test]
    fn move_valve_segment_fields_round_trip() {
        let cmd = MoveValveSegment {
            valve_id: 2,
            t_start: 0.2,
            t_end: 0.8,
        };
        assert_eq!(cmd.valve_id, 2);
        assert!((cmd.t_start - 0.2).abs() < f32::EPSILON);
        assert!((cmd.t_end - 0.8).abs() < f32::EPSILON);
    }
}
