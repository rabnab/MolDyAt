//! Line-segment operator commands.

use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;
use crate::objects2d::Vec2D;

/// Add a new line-segment barrier from `start` to `end`.
#[derive(Debug)]
pub struct AddLineSegment {
    pub start: Vec2D,
    pub end: Vec2D,
}

impl OperatorCommand for AddLineSegment {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.add_line_segment(self.start, self.end);
    }
}

/// Move one endpoint of an existing line segment.
#[derive(Debug)]
pub struct MoveLineSegmentEndpoint {
    pub segment_id: usize,
    pub new_pos: Vec2D,
    /// `true` → move start endpoint, `false` → move end endpoint.
    pub move_start: bool,
}

impl OperatorCommand for MoveLineSegmentEndpoint {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.move_line_segment_endpoint(self.segment_id, self.new_pos, self.move_start);
    }
}

#[derive(Debug)]
pub struct ChangeLineThickness {
    pub segment_id: usize,
    pub steps: i32,
}

impl OperatorCommand for ChangeLineThickness {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.change_thickness(self.segment_id, self.steps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_line_segment_fields_round_trip() {
        let cmd = AddLineSegment {
            start: Vec2D::new(1.0, 2.0),
            end: Vec2D::new(3.0, 4.0),
        };
        assert_eq!(cmd.start, Vec2D::new(1.0, 2.0));
        assert_eq!(cmd.end, Vec2D::new(3.0, 4.0));
    }

    #[test]
    fn move_line_segment_endpoint_fields_round_trip() {
        let cmd = MoveLineSegmentEndpoint {
            segment_id: 7,
            new_pos: Vec2D::new(5.0, 6.0),
            move_start: true,
        };
        assert_eq!(cmd.segment_id, 7);
        assert_eq!(cmd.new_pos, Vec2D::new(5.0, 6.0));
        assert!(cmd.move_start);
    }
}
