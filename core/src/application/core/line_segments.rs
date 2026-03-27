use super::MainContent;
use crate::objects2d::{SimLineSegment, Vec2D};

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

    /// Move the closest endpoint of a line segment to a new position.
    pub(crate) fn move_line_segment_endpoint(&mut self, segment_id: usize, new_pos: Vec2D) {
        if let Some(seg) = self.line_segments.iter_mut().find(|s| s.id == segment_id) {
            let d_start = seg.start.distance_squared(&new_pos);
            let d_end = seg.end.distance_squared(&new_pos);
            if d_start <= d_end {
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

        // Iterate over each segment; for each ball check collision.
        // We need &self.line_segments and &mut self.balls simultaneously,
        // so collect segment data first.
        let segments: Vec<SimLineSegment> = self.line_segments.clone();

        for seg in &segments {
            if seg.length_sq() < 1e-12 {
                continue; // degenerate segment
            }
            for ball in &mut self.balls {
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
}

#[cfg(test)]
mod tests {
    use crate::objects2d::Vec2D;
    use crate::objects2d::ball_impl::Ball2D;

    /// Helper: create a MainContent with no balls and a default world.
    fn make_content() -> super::MainContent {
        super::MainContent::new(crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0)).unwrap()
    }

    #[test]
    fn add_and_clear_segments() {
        let mut mc = make_content();
        mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        mc.add_line_segment(Vec2D::new(0.0, 5.0), Vec2D::new(10.0, 5.0));
        assert_eq!(mc.line_segments.len(), 2);
        assert_eq!(mc.line_segments[0].id, 0);
        assert_eq!(mc.line_segments[1].id, 1);

        mc.clear_line_segments();
        assert!(mc.line_segments.is_empty());
        assert_eq!(mc.line_segment_next_id, 0);
    }

    #[test]
    fn move_endpoint_closest() {
        let mut mc = make_content();
        mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        // Move start (closer to (0.5, 0))
        mc.move_line_segment_endpoint(0, Vec2D::new(0.5, 0.0));
        assert!((mc.line_segments[0].start.x - 0.5).abs() < 1e-6);
    }

    #[test]
    fn collision_reflects_ball() {
        let mut mc = make_content();
        // Horizontal segment at y=0 from x=-10 to x=10
        mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));

        // Ball at y=0.5, radius=1.0, moving downward
        let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.5), (0, 1.0));
        ball.v = Vec2D::new(0.0, -5.0);
        mc.balls.push(ball);

        let (_px, py) = mc.resolve_line_segment_collisions();

        // Ball should now be moving upward
        assert!(mc.balls[0].v.y > 0.0, "Ball should reflect upward");
        // Speed should be preserved (elastic)
        assert!((mc.balls[0].v.norm() - 5.0).abs() < 0.1);
        // Some momentum should be transferred
        assert!(py.abs() > 0.0);
    }

    #[test]
    fn no_collision_when_moving_away() {
        let mut mc = make_content();
        mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));

        // Ball overlapping but moving away
        let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.5), (0, 1.0));
        ball.v = Vec2D::new(0.0, 5.0); // moving away from segment
        mc.balls.push(ball);

        let _ = mc.resolve_line_segment_collisions();

        // Should NOT reflect (velocity gate)
        assert!(mc.balls[0].v.y > 0.0);
    }
}
