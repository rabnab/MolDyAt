use super::MainContent;
use crate::objects2d::operator::{is_degenerate, remap_point};
use crate::objects2d::{Rect, SimLineSegment, Vec2D};

/// Collision tolerance: balls closer than `radius + EPSILON` are considered colliding.
const COLLISION_EPSILON: f32 = 0.01;
/// Velocity gate: only reflect if ball is moving toward the segment.
const VELOCITY_GATE_EPSILON: f32 = -1e-6;

impl MainContent {
    /// Add a new line segment barrier.
    pub(crate) fn add_line_segment(&mut self, start: Vec2D, end: Vec2D) {
        let id = self.line_segment_next_id;
        self.line_segment_next_id += 1;
        self.line_segments.push(SimLineSegment::new(id, start, end));
    }

    /// Remove all line segments and reset ID counter.
    pub(crate) fn clear_line_segments(&mut self) {
        self.line_segments.clear();
        self.line_segment_next_id = 0;
    }

    /// Move a specific endpoint of a line segment to a new position.
    ///
    /// `move_start`: `true` → move the start endpoint, `false` → move the end.
    pub(crate) fn move_line_segment_endpoint(
        &mut self,
        segment_id: usize,
        new_pos: Vec2D,
        move_start: bool,
    ) {
        if let Some(seg) = self.line_segments.iter_mut().find(|s| s.id == segment_id) {
            if move_start {
                seg.start = new_pos;
            } else {
                seg.end = new_pos;
            }
        }
    }

    /// Resolve ball–line-segment collisions for all balls.
    ///
    /// For each ball that overlaps a segment (surface distance < 0) and is
    /// moving toward it (`v · n < 0`), the ball is:
    /// 1. Position-corrected so it no longer overlaps.
    /// 2. Velocity-reflected: `v' = v − 2(v · n̂) n̂` (elastic, |v| preserved).
    ///
    /// Returns accumulated wall momentum `(px, py)` for pressure tracking.
    pub(crate) fn resolve_line_segment_collisions(&mut self) -> (f32, f32) {
        let mut total_px = 0.0_f32;
        let mut total_py = 0.0_f32;

        // Split borrow: &self.line_segments (immutable) and &mut self.balls
        // (mutable) are disjoint fields, so no clone needed.
        for seg in &self.line_segments {
            if seg.length_sq() < 1e-12 {
                continue; // degenerate segment
            }

            let open_ranges: Vec<(f32, f32)> = self
                .valve_segments
                .iter()
                .filter(|v| v.line_id == seg.id && !v.is_closed)
                .map(|v| (v.t_start, v.t_end))
                .collect();

            for ball in &mut self.balls {
                let t = seg.closest_t(ball.get_position());
                if open_ranges.iter().any(|(a, b)| t >= *a && t <= *b) {
                    continue;
                }

                let closest = seg.closest_point(ball.get_position());
                let to_ball = closest.dist(ball.get_position());
                let center_dist = to_ball.norm();

                if center_dist >= ball.radius + COLLISION_EPSILON {
                    continue; // no collision
                }
                if center_dist < 1e-12 {
                    continue; // ball center exactly on segment, skip
                }

                let normal = to_ball.scaled(1.0 / center_dist);

                // Velocity gate: only reflect if moving toward the segment
                let v_dot_n = ball.v.x * normal.x + ball.v.y * normal.y;
                if v_dot_n > VELOCITY_GATE_EPSILON {
                    continue; // moving away or parallel
                }

                // Position correction: push ball out so surface touches segment
                let overlap = ball.radius + COLLISION_EPSILON - center_dist;
                let corrected_pos = ball.get_position().add_scaled(&normal, overlap);
                ball.set_position_and_update_tail(corrected_pos);

                // Elastic reflection: v' = v − 2(v·n̂)n̂
                ball.v.x -= 2.0 * v_dot_n * normal.x;
                ball.v.y -= 2.0 * v_dot_n * normal.y;

                // Accumulate wall momentum for pressure calculation
                total_px += 2.0 * ball.mass * v_dot_n * normal.x;
                total_py += 2.0 * ball.mass * v_dot_n * normal.y;
            }
        }

        (total_px, total_py)
    }

    /// Get the current line segments (for snapshot creation).
    pub(crate) fn get_line_segments(&self) -> &[SimLineSegment] {
        &self.line_segments
    }

    /// Rescale all line segment endpoints so they keep their relative
    /// position when the world region changes size.
    pub(crate) fn rescale_line_segments(&mut self, old_rect: &Rect, new_rect: &Rect) {
        if is_degenerate(old_rect) {
            return;
        }
        for seg in &mut self.line_segments {
            seg.start = remap_point(seg.start, old_rect, new_rect);
            seg.end = remap_point(seg.end, old_rect, new_rect);
        }
    }
}
