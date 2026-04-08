use super::MainContent;
use crate::objects2d::{Rect, ValveSegment};

impl MainContent {
    /// Add a valve section on a parent line segment.
    pub(crate) fn add_valve_segment(
        &mut self,
        line_id: usize,
        t_start: f32,
        t_end: f32,
        is_closed: bool,
    ) {
        if !self.line_segments.iter().any(|s| s.id == line_id) {
            return;
        }
        let id = self.valve_segment_next_id;
        self.valve_segment_next_id += 1;
        self.valve_segments
            .push(ValveSegment::new(id, line_id, t_start, t_end, is_closed));
    }

    /// Toggle a valve between open and closed states.
    ///
    /// If toggled to closed, overlapping balls are pushed to the nearest
    /// non-overlapping side of the parent segment.
    pub(crate) fn set_valve_closed(&mut self, valve_id: usize, is_closed: bool) {
        let Some(idx) = self.valve_segments.iter().position(|v| v.id == valve_id) else {
            return;
        };
        self.valve_segments[idx].is_closed = is_closed;

        if !is_closed {
            return;
        }

        let valve = self.valve_segments[idx];
        let Some(seg) = self
            .line_segments
            .iter()
            .find(|s| s.id == valve.line_id)
            .copied()
        else {
            return;
        };

        // Resolve immediate overlap after closing the valve.
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

    /// Toggle a valve between open and closed.
    pub(crate) fn toggle_valve(&mut self, valve_id: usize) {
        let Some(idx) = self.valve_segments.iter().position(|v| v.id == valve_id) else {
            return;
        };
        let new_closed = !self.valve_segments[idx].is_closed;
        self.set_valve_closed(valve_id, new_closed);
    }

    /// Move an existing valve section along the parent segment.
    pub(crate) fn move_valve_segment(&mut self, valve_id: usize, t_start: f32, t_end: f32) {
        if let Some(v) = self.valve_segments.iter_mut().find(|v| v.id == valve_id) {
            let updated = ValveSegment::new(v.id, v.line_id, t_start, t_end, v.is_closed);
            *v = updated;
        }
    }

    /// Remove all valve segments and reset ID counter.
    pub(crate) fn clear_valves(&mut self) {
        self.valve_segments.clear();
        self.valve_segment_next_id = 0;
    }

    /// Rescale valves for world resize.
    ///
    /// Valve sections are stored as parametric coordinates on parent line
    /// segments, so they naturally follow line endpoint remapping. We keep this
    /// method for symmetric operator lifecycle handling.
    pub(crate) fn rescale_valves(&mut self, _old_rect: &Rect, _new_rect: &Rect) {
        // Valve geometry is parametric on line segments (t_start/t_end), so
        // line endpoint rescaling is sufficient and no direct valve mutation is needed.
    }
}
