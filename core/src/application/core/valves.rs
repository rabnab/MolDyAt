use super::{MainContent, SimulationWorld};
use crate::application::renderer_snapshot::ValveSpan;
use crate::objects2d::wall_chain::Joint;

impl MainContent {
    /// Add a valve on wall piece `line_id`, given as piece-local fractions
    /// `t_start`/`t_end`. Splits the piece; see `SimulationWorld::add_valve_on_piece`.
    pub(crate) fn add_valve_segment(
        &mut self,
        line_id: usize,
        t_start: f32,
        t_end: f32,
        is_closed: bool,
    ) {
        self.world
            .add_valve_on_piece(line_id, t_start, t_end, is_closed);
    }

    /// Open or close a valve. Balls inside it when it closes are not moved;
    /// they leave through either side on their own.
    pub(crate) fn set_valve_closed(&mut self, valve_id: usize, is_closed: bool) {
        self.world.set_valve_closed(valve_id, is_closed);
    }

    /// Toggle a valve between open and closed.
    pub(crate) fn toggle_valve(&mut self, valve_id: usize) {
        self.world.toggle_valve(valve_id);
    }
}

impl SimulationWorld {
    pub(crate) fn set_valve_closed(&mut self, valve_id: usize, is_closed: bool) {
        let Some(idx) = self.valve_segments.position(valve_id) else {
            return;
        };
        self.valve_segments.get_mut(idx).is_closed = is_closed;
        // Balls in the span when it closes are not moved: they leave on
        // their own (see `resolve_line_segment_collisions`).
        if is_closed {
            self.flag_balls_overlapping_valve(valve_id);
        }
    }

    pub(crate) fn toggle_valve(&mut self, valve_id: usize) {
        let Some(idx) = self.valve_segments.position(valve_id) else {
            return;
        };
        let new_closed = !self.valve_segments.get(idx).is_closed;
        self.set_valve_closed(valve_id, new_closed);
    }

    /// Resolved world-space geometry of every valve, in chain order.
    ///
    /// Computed once per snapshot so renderer and hit-testing never repeat
    /// the chain math.
    pub(crate) fn valve_spans(&self) -> Vec<ValveSpan> {
        let mut spans = Vec::new();
        for chain in self.wall_chains.chains() {
            for (index, joint) in chain.joints.iter().enumerate() {
                let Joint::Valve {
                    valve_id,
                    t_start,
                    t_end,
                } = joint
                else {
                    continue;
                };
                let Some(valve) = self.valve_segments.find(*valve_id) else {
                    continue;
                };
                spans.push(ValveSpan {
                    valve_id: *valve_id,
                    p0: chain.point_at(*t_start),
                    p1: chain.point_at(*t_end),
                    thickness: self.joint_thickness(chain, index),
                    is_closed: valve.is_closed,
                });
            }
        }
        spans
    }
}
