//! Line segment operator for impenetrable barriers.
//!
//! A `SimLineSegment` represents a static barrier that balls reflect off
//! elastically.  Line segments are always active (no ON/OFF toggle) and
//! are removed when the simulation is reset.

use super::Vec2D;

/// A line segment barrier in simulation space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimLineSegment {
    /// Unique identifier (monotonically increasing, reset on clear).
    pub id: usize,
    /// Start point in world coordinates.
    pub start: Vec2D,
    /// End point in world coordinates.
    pub end: Vec2D,
}

impl SimLineSegment {
    /// Create a new line segment.
    pub fn new(id: usize, start: Vec2D, end: Vec2D) -> Self {
        Self { id, start, end }
    }

    /// Direction vector from start to end.
    pub fn direction(&self) -> Vec2D {
        self.start.dist(&self.end)
    }

    /// Squared length of the segment.
    pub fn length_sq(&self) -> f32 {
        self.direction().norm_sq()
    }

    /// Closest point on this segment to `point`.
    ///
    /// Clamps the projection parameter `t` to `[0, 1]` so the result
    /// always lies on the segment, not the infinite line.
    pub fn closest_point(&self, point: &Vec2D) -> Vec2D {
        let d = self.direction();
        let len_sq = d.norm_sq();
        if len_sq < 1e-12 {
            return self.start;
        }
        let ap = self.start.dist(point);
        let t = (ap.x * d.x + ap.y * d.y) / len_sq;
        let t = t.clamp(0.0, 1.0);
        self.start.add_scaled(&d, t)
    }

    /// Outward normal toward `point`.
    ///
    /// Returns the unit normal of the segment that faces `point`.
    pub fn normal_toward(&self, point: &Vec2D) -> Vec2D {
        let d = self.direction();
        // Perpendicular candidates: (-dy, dx) and (dy, -dx)
        let n = Vec2D::new(-d.y, d.x);
        let closest = self.closest_point(point);
        let to_point = closest.dist(point);
        if n.x * to_point.x + n.y * to_point.y >= 0.0 {
            n.normalize()
        } else {
            Vec2D::new(d.y, -d.x).normalize()
        }
    }

    /// Signed distance from ball surface to this segment.
    ///
    /// Negative means the ball overlaps the segment.
    pub fn distance_to_ball(&self, ball_center: &Vec2D, ball_radius: f32) -> f32 {
        let closest = self.closest_point(ball_center);
        let center_dist = closest.dist(ball_center).norm();
        center_dist - ball_radius
    }
}

/// Snap `pos` to the nearest simulation wall if within `threshold`.
///
/// Wall snap takes priority over endpoint snap (per issue spec).
pub fn snap_to_walls(pos: Vec2D, world_rect: &super::Rect, threshold: f32) -> Vec2D {
    let (x0, y0) = world_rect.get_tl();
    let (x1, y1) = world_rect.get_br();
    let min_x = f32::min(x0, x1);
    let max_x = f32::max(x0, x1);
    let min_y = f32::min(y0, y1);
    let max_y = f32::max(y0, y1);

    let mut result = pos;
    if (pos.x - min_x).abs() < threshold {
        result.x = min_x;
    } else if (pos.x - max_x).abs() < threshold {
        result.x = max_x;
    }
    if (pos.y - min_y).abs() < threshold {
        result.y = min_y;
    } else if (pos.y - max_y).abs() < threshold {
        result.y = max_y;
    }
    result
}

/// Snap `pos` to the nearest existing segment endpoint if within `threshold`.
///
/// Returns the snapped position (unchanged if nothing is close enough).
pub fn snap_to_endpoints(pos: Vec2D, segments: &[SimLineSegment], threshold: f32) -> Vec2D {
    let threshold_sq = threshold * threshold;
    let mut best_dist_sq = threshold_sq;
    let mut best = pos;

    for seg in segments {
        for &ep in &[seg.start, seg.end] {
            let d_sq = pos.distance_squared(&ep);
            if d_sq < best_dist_sq {
                best_dist_sq = d_sq;
                best = ep;
            }
        }
    }
    best
}

/// Combined snap: walls first, then endpoints.
pub fn snap_position(
    pos: Vec2D,
    world_rect: &super::Rect,
    segments: &[SimLineSegment],
    threshold: f32,
) -> Vec2D {
    let wall_snapped = snap_to_walls(pos, world_rect, threshold);
    if wall_snapped != pos {
        return wall_snapped;
    }
    snap_to_endpoints(pos, segments, threshold)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects2d::Rect;

    #[test]
    fn closest_point_at_start() {
        let seg = SimLineSegment::new(0, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let p = seg.closest_point(&Vec2D::new(-5.0, 3.0));
        assert!((p.x).abs() < 1e-6);
        assert!((p.y).abs() < 1e-6);
    }

    #[test]
    fn closest_point_at_end() {
        let seg = SimLineSegment::new(0, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let p = seg.closest_point(&Vec2D::new(15.0, 3.0));
        assert!((p.x - 10.0).abs() < 1e-6);
        assert!((p.y).abs() < 1e-6);
    }

    #[test]
    fn closest_point_midpoint() {
        let seg = SimLineSegment::new(0, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let p = seg.closest_point(&Vec2D::new(5.0, 7.0));
        assert!((p.x - 5.0).abs() < 1e-6);
        assert!((p.y).abs() < 1e-6);
    }

    #[test]
    fn normal_faces_point() {
        let seg = SimLineSegment::new(0, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let n = seg.normal_toward(&Vec2D::new(5.0, 3.0));
        // Normal should point upward (positive y direction toward the point)
        assert!(n.y > 0.0);
        assert!((n.norm() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn normal_faces_point_below() {
        let seg = SimLineSegment::new(0, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let n = seg.normal_toward(&Vec2D::new(5.0, -3.0));
        assert!(n.y < 0.0);
    }

    #[test]
    fn distance_to_ball_positive() {
        let seg = SimLineSegment::new(0, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let d = seg.distance_to_ball(&Vec2D::new(5.0, 5.0), 1.0);
        assert!((d - 4.0).abs() < 1e-5);
    }

    #[test]
    fn distance_to_ball_overlapping() {
        let seg = SimLineSegment::new(0, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let d = seg.distance_to_ball(&Vec2D::new(5.0, 0.5), 1.0);
        assert!(d < 0.0); // overlapping
    }

    #[test]
    fn snap_to_wall_x() {
        let rect = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let snapped = snap_to_walls(Vec2D::new(-9.7, 0.0), &rect, 0.5);
        assert!((snapped.x - (-10.0)).abs() < 1e-6);
    }

    #[test]
    fn snap_to_endpoint() {
        let seg = SimLineSegment::new(0, Vec2D::new(5.0, 5.0), Vec2D::new(10.0, 5.0));
        let snapped = snap_to_endpoints(Vec2D::new(5.3, 5.2), &[seg], 0.5);
        assert!((snapped.x - 5.0).abs() < 1e-6);
        assert!((snapped.y - 5.0).abs() < 1e-6);
    }

    #[test]
    fn snap_wall_takes_priority() {
        let rect = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let seg = SimLineSegment::new(0, Vec2D::new(-9.8, 0.0), Vec2D::new(0.0, 0.0));
        let pos = Vec2D::new(-9.7, 0.0);
        let snapped = snap_position(pos, &rect, &[seg], 0.5);
        // Wall snap should win
        assert!((snapped.x - (-10.0)).abs() < 1e-6);
    }
}
