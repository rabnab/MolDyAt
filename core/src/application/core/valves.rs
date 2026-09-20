use super::{MainContent, SimulationWorld};
use crate::application::renderer_snapshot::ValveSpan;
use crate::objects2d::SimLineSegment;
use crate::objects2d::line_segment::THICKNESS_TO_WORLD;
use crate::objects2d::wall_chain::Joint;

/// Extra clearance when pushing balls out of a freshly closed valve.
const CLOSE_PUSH_EPSILON: f32 = 0.01;

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

    /// Toggle a valve between open and closed states.
    ///
    /// If toggled to closed, overlapping balls are pushed out of the valve.
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

        if !is_closed {
            return;
        }

        let Some(thickness) = self.valve_thickness(valve_id) else {
            return;
        };
        let Some((p0, p1)) = self.wall_chains.valve_span(valve_id) else {
            return;
        };
        let span = SimLineSegment::new(0, p0, p1);
        let half_thickness = 0.5 * thickness * THICKNESS_TO_WORLD;

        for ball in &mut self.balls {
            let closest = span.closest_point(ball.get_position());
            let to_ball = closest.dist(ball.get_position());
            let center_dist = to_ball.norm();
            let min_dist = ball.radius + half_thickness + CLOSE_PUSH_EPSILON;
            if center_dist >= min_dist || center_dist < 1e-12 {
                continue;
            }
            let normal = to_ball.scaled(1.0 / center_dist);
            let overlap = min_dist - center_dist;
            let corrected = ball.get_position().add_scaled(&normal, overlap);
            ball.set_position_and_update_tail(corrected);
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
