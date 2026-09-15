use super::{MainContent, SimulationWorld};
use crate::objects2d::Vec2D;
// Single source of truth for thickness units, shared with the renderer.
// Add to objects2d/line_segment.rs:  pub const THICKNESS_TO_WORLD: f32 = 0.05;
use crate::objects2d::line_segment::THICKNESS_TO_WORLD;

/// Collision tolerance: balls closer than `radius + half_thickness + EPSILON`
/// to a wall's centre line are considered colliding.
const COLLISION_EPSILON: f32 = 0.01;
/// Velocity gate: only reflect if ball is moving toward the segment.
const VELOCITY_GATE_EPSILON: f32 = -1e-6;

const THICKNESS_STEP: f32 = 1.0;

impl MainContent {
    /// Add a new line segment barrier.
    pub(crate) fn add_line_segment(&mut self, start: Vec2D, end: Vec2D) {
        self.world.add_line_segment(start, end);
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
        self.world
            .move_line_segment_endpoint(segment_id, new_pos, move_start);
    }

    pub(crate) fn change_thickness(&mut self, segment_id: usize, steps: i32) {
        self.world.change_line_thickness(segment_id, steps);
    }

    /// Resolve ball–line-segment collisions for all balls.
    ///
    /// Each wall is a capsule: its centre line inflated by half its thickness
    /// (`thickness * THICKNESS_TO_WORLD / 2`), with rounded ends. For each ball
    /// that overlaps a wall (centre-line distance < radius + half thickness) and
    /// is moving toward it (`v · n < 0`), the ball is:
    /// 1. Position-corrected so it no longer overlaps.
    /// 2. Velocity-reflected: `v' = v − 2(v · n̂) n̂` (elastic, |v| preserved).
    ///
    /// Returns accumulated wall momentum `(px, py)` for pressure tracking.
    pub(crate) fn resolve_line_segment_collisions(&mut self) -> (f32, f32) {
        self.world.resolve_line_segment_collisions()
    }
}

impl SimulationWorld {
    pub(crate) fn add_line_segment(&mut self, start: Vec2D, end: Vec2D) {
        self.line_segments.add(start, end);
    }

    pub(crate) fn move_line_segment_endpoint(
        &mut self,
        segment_id: usize,
        new_pos: Vec2D,
        move_start: bool,
    ) {
        if let Some(seg) = self.line_segments.find_mut(segment_id) {
            if move_start {
                seg.start = new_pos;
            } else {
                seg.end = new_pos;
            }
        }
    }

    pub(crate) fn change_line_thickness(&mut self, segment_id: usize, steps: i32) {
        if let Some(seg) = self.line_segments.find_mut(segment_id) {
            seg.thickness = f32::clamp(seg.thickness + steps as f32 * THICKNESS_STEP, 1.0, 20.0);
        }
    }

    pub(crate) fn resolve_line_segment_collisions(&mut self) -> (f32, f32) {
        let mut total_px = 0.0_f32;
        let mut total_py = 0.0_f32;

        let (line_segments, valve_segments, balls) =
            (&self.line_segments, &self.valve_segments, &mut self.balls);
        let segments = line_segments.segments();
        let valves = valve_segments.valves();

        for seg in segments {
            if seg.length_sq() < 1e-12 {
                continue; // degenerate segment
            }

            // The wall is a capsule: the centre line inflated by half its thickness.
            // closest_point() clamps to the endpoints, so the ends are rounded,
            // matching what wall.wgsl draws.
            let half_thickness = 0.5 * seg.thickness * THICKNESS_TO_WORLD;

            for ball in balls.iter_mut() {
                let t = seg.closest_t(ball.get_position());
                if valves
                    .iter()
                    .any(|v| v.line_id == seg.id && !v.is_closed && t >= v.t_start && t <= v.t_end)
                {
                    continue;
                }

                let closest = seg.closest_point(ball.get_position());
                let to_ball = closest.dist(ball.get_position());
                let center_dist = to_ball.norm();

                // Ball touches the wall surface when its centre is closer to the
                // centre line than this.
                let reach = ball.radius + half_thickness + COLLISION_EPSILON;
                if center_dist >= reach {
                    continue; // no collision
                }

                let normal = if center_dist > 1e-12 {
                    to_ball.scaled(1.0 / center_dist)
                } else {
                    // Centre exactly on the centre line (possible with a thick wall):
                    // use the wall's perpendicular, on the side opposing the velocity.
                    let len = seg.length_sq().sqrt();
                    let n = Vec2D::new(
                        -(seg.end.y - seg.start.y) / len,
                        (seg.end.x - seg.start.x) / len,
                    );
                    if n.x * ball.v.x + n.y * ball.v.y > 0.0 {
                        Vec2D::new(-n.x, -n.y)
                    } else {
                        n
                    }
                };

                // Velocity gate: only reflect if moving toward the segment
                let v_dot_n = ball.v.x * normal.x + ball.v.y * normal.y;
                if v_dot_n > VELOCITY_GATE_EPSILON {
                    continue; // moving away or parallel
                }

                // Position correction: push ball out so its surface touches the wall surface
                let overlap = reach - center_dist;
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
}
