use crate::application::MainContent;
use crate::objects2d::Vec2D;

/// A self-contained operator command that knows how to apply itself.
///
/// Operator commands encapsulate all data needed for a single operator
/// mutation. New operator types define concrete structs implementing this
/// trait — no changes to `PhysicsCommand`, `InputAction`, or the action
/// processor are required.
pub trait OperatorCommand: Send + std::fmt::Debug {
    fn execute(self: Box<Self>, content: &mut MainContent);
}

// ── Line-segment commands ────────────────────────────────────────────

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

// ── Valve commands ───────────────────────────────────────────────────

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

#[derive(Debug)]
pub struct ToggleValve {
    pub valve_id: usize,
}

impl OperatorCommand for ToggleValve {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.toggle_valve(self.valve_id);
    }
}

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
