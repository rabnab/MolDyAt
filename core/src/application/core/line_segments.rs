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

        // Split borrow: &self.line_segments (immutable) and &mut self.balls
        // (mutable) are disjoint fields, so no clone needed.
        for seg in &self.line_segments {
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

    // ── Rescale tests ───────────────────────────────────────────────

    #[test]
    fn rescale_preserves_relative_position() {
        let mut mc = make_content();
        // World is (-20, 20, 40, -40) → x: -20..20, y: 20..-20
        // Segment at (0, 0) → (10, -10) means (50%, 50%) → (75%, 75%)
        mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(10.0, -10.0));

        let old_rect = crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0);
        // Double horizontal size: x: -40..40
        let new_rect = crate::objects2d::Rect::new(-40.0, 20.0, 80.0, -40.0);
        mc.rescale_line_segments(&old_rect, &new_rect);

        // 50% of new width 80 from -40 → 0.0 → new x = -40 + 0.5*80 = 0.0 (unchanged)
        assert!((mc.line_segments[0].start.x - 0.0).abs() < 1e-5);
        assert!((mc.line_segments[0].start.y - 0.0).abs() < 1e-5);
        // 75% of new width 80 from -40 → new x = -40 + 0.75*80 = 20.0
        assert!((mc.line_segments[0].end.x - 20.0).abs() < 1e-5);
        // y unchanged (same height)
        assert!((mc.line_segments[0].end.y - (-10.0)).abs() < 1e-5);
    }

    #[test]
    fn rescale_both_axes() {
        let mut mc = make_content();
        // Midpoint of rect: (50%, 50%) → (0, 0) in (-20,20,40,-40)
        mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(0.0, 0.0));

        let old_rect = crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0);
        // New rect: (-10, 10, 20, -20) — half size in both axes
        let new_rect = crate::objects2d::Rect::new(-10.0, 10.0, 20.0, -20.0);
        mc.rescale_line_segments(&old_rect, &new_rect);

        // 50% of new rect → midpoint (0, 0)
        assert!((mc.line_segments[0].start.x - 0.0).abs() < 1e-5);
        assert!((mc.line_segments[0].start.y - 0.0).abs() < 1e-5);
    }

    #[test]
    fn rescale_corner_endpoints() {
        let mut mc = make_content();
        // Top-left corner of world (-20, 20) → (0%, 0%)
        mc.add_line_segment(Vec2D::new(-20.0, 20.0), Vec2D::new(20.0, -20.0));

        let old_rect = crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0);
        let new_rect = crate::objects2d::Rect::new(-40.0, 40.0, 80.0, -80.0);
        mc.rescale_line_segments(&old_rect, &new_rect);

        // 0% → top-left of new rect
        assert!((mc.line_segments[0].start.x - (-40.0)).abs() < 1e-5);
        assert!((mc.line_segments[0].start.y - 40.0).abs() < 1e-5);
        // 100% → bottom-right of new rect
        assert!((mc.line_segments[0].end.x - 40.0).abs() < 1e-5);
        assert!((mc.line_segments[0].end.y - (-40.0)).abs() < 1e-5);
    }

    #[test]
    fn rescale_skips_degenerate_old_rect() {
        let mut mc = make_content();
        mc.add_line_segment(Vec2D::new(5.0, 5.0), Vec2D::new(10.0, 10.0));

        let old_rect = crate::objects2d::Rect::new(0.0, 0.0, 0.0, 0.0);
        let new_rect = crate::objects2d::Rect::new(-10.0, 10.0, 20.0, -20.0);
        mc.rescale_line_segments(&old_rect, &new_rect);

        // Endpoints unchanged (degenerate old rect)
        assert!((mc.line_segments[0].start.x - 5.0).abs() < 1e-5);
        assert!((mc.line_segments[0].end.x - 10.0).abs() < 1e-5);
    }

    #[test]
    fn rescale_multiple_segments() {
        let mut mc = make_content();
        mc.add_line_segment(Vec2D::new(-20.0, 0.0), Vec2D::new(0.0, 0.0));
        mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(20.0, 0.0));

        let old_rect = crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0);
        let new_rect = crate::objects2d::Rect::new(-40.0, 20.0, 80.0, -40.0);
        mc.rescale_line_segments(&old_rect, &new_rect);

        // First segment: 0% → 50%  mapped to new width
        assert!((mc.line_segments[0].start.x - (-40.0)).abs() < 1e-5);
        assert!((mc.line_segments[0].end.x - 0.0).abs() < 1e-5);
        // Second segment: 50% → 100%
        assert!((mc.line_segments[1].start.x - 0.0).abs() < 1e-5);
        assert!((mc.line_segments[1].end.x - 40.0).abs() < 1e-5);
    }
}
