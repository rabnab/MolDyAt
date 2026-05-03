use super::{MainContent, SimulationWorld};
use crate::objects2d::ValveSegment;

impl MainContent {
    /// Add a valve section on a parent line segment.
    pub(crate) fn add_valve_segment(
        &mut self,
        line_id: usize,
        t_start: f32,
        t_end: f32,
        is_closed: bool,
    ) {
        self.world
            .add_valve_segment(line_id, t_start, t_end, is_closed);
    }

    /// Toggle a valve between open and closed states.
    ///
    /// If toggled to closed, overlapping balls are pushed to the nearest
    /// non-overlapping side of the parent segment.
    pub(crate) fn set_valve_closed(&mut self, valve_id: usize, is_closed: bool) {
        self.world.set_valve_closed(valve_id, is_closed);
    }

    /// Toggle a valve between open and closed.
    pub(crate) fn toggle_valve(&mut self, valve_id: usize) {
        self.world.toggle_valve(valve_id);
    }

    /// Move an existing valve section along the parent segment.
    pub(crate) fn move_valve_segment(&mut self, valve_id: usize, t_start: f32, t_end: f32) {
        self.world.move_valve_segment(valve_id, t_start, t_end);
    }
}

impl SimulationWorld {
    pub(crate) fn add_valve_segment(
        &mut self,
        line_id: usize,
        t_start: f32,
        t_end: f32,
        is_closed: bool,
    ) {
        if self.line_segments.find(line_id).is_none() {
            return;
        }
        self.valve_segments.add(line_id, t_start, t_end, is_closed);
    }

    pub(crate) fn set_valve_closed(&mut self, valve_id: usize, is_closed: bool) {
        let Some(idx) = self.valve_segments.position(valve_id) else {
            return;
        };
        self.valve_segments.get_mut(idx).is_closed = is_closed;

        if !is_closed {
            return;
        }

        let valve = *self.valve_segments.get(idx);
        let Some(seg) = self.line_segments.find(valve.line_id).copied() else {
            return;
        };

        for ball in &mut self.balls {
            let t = seg.closest_t(ball.get_position());
            if !valve.contains_t(t) {
                continue;
            }
            let closest = seg.closest_point(ball.get_position());
            let to_ball = closest.dist(ball.get_position());
            let center_dist = to_ball.norm();
            let min_dist = ball.radius + 0.01;
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

    pub(crate) fn move_valve_segment(&mut self, valve_id: usize, t_start: f32, t_end: f32) {
        if let Some(v) = self.valve_segments.find_mut(valve_id) {
            let updated = ValveSegment::new(v.id, v.line_id, t_start, t_end, v.is_closed);
            *v = updated;
        }
    }
}
