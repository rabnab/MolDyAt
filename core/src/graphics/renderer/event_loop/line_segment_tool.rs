//! Line segment operator tool — drawing, snapping, and endpoint dragging.
//!
//! This module extends [`UnifiedRenderState`] with the line-segment
//! drawing state machine, snap-to-wall / snap-to-endpoint helpers,
//! ghost-preview queries, and the endpoint find/drag interaction.

use crate::objects2d::Vec2D;
use crate::objects2d::line_segment::snap_position;

use super::input_handler::OperatorTool;
use super::unified_render_state::UnifiedRenderState;

/// State machine for drawing a line segment.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LineSegmentDrawState {
    /// Waiting for the user to press left mouse button.
    #[default]
    Idle,
    /// Left button pressed; dragging from `start` to current cursor.
    Drawing { start: Vec2D },
    /// Dragging an existing segment endpoint to reposition it.
    MovingEndpoint {
        segment_id: usize,
        /// `true` when the grabbed endpoint is `start`.
        move_start: bool,
    },
}

impl UnifiedRenderState {
    // ── Line segment snapping ────────────────────────────────────────

    /// Snap threshold in world units.
    pub(super) const SNAP_THRESHOLD: f32 = 0.5;

    /// Snap a world position to nearby walls or existing segment endpoints.
    pub(super) fn snap_world_pos(&self, pos: Vec2D) -> Vec2D {
        let world_rect = self.viewport.world_bounds();
        let segments: &[crate::objects2d::SimLineSegment] = self
            .last_snapshot
            .as_ref()
            .map(|s| s.operators.line_segments.as_slice())
            .unwrap_or(&[]);
        snap_position(pos, &world_rect, segments, Self::SNAP_THRESHOLD)
    }

    /// Find a segment whose endpoint is within [`SNAP_THRESHOLD`] of `pos`.
    ///
    /// Returns `(segment_id, move_start)` where `move_start` is `true`
    /// when the closest endpoint is the segment's start point.
    pub(super) fn find_near_endpoint(&self, pos: Vec2D) -> Option<(usize, bool)> {
        let threshold_sq = Self::SNAP_THRESHOLD * Self::SNAP_THRESHOLD;
        let segments = self
            .last_snapshot
            .as_ref()
            .map(|s| s.operators.line_segments.as_slice())
            .unwrap_or(&[]);
        let mut best_dist_sq = threshold_sq;
        let mut best: Option<(usize, bool)> = None;
        for seg in segments {
            let d_start = pos.distance_squared(&seg.start);
            let d_end = pos.distance_squared(&seg.end);
            if d_start < best_dist_sq {
                best_dist_sq = d_start;
                best = Some((seg.id, true));
            }
            if d_end < best_dist_sq {
                best_dist_sq = d_end;
                best = Some((seg.id, false));
            }
        }
        best
    }

    // ── Line segment preview ────────────────────────────────────────

    /// Returns the current ghost-preview line segment (start, end in world
    /// coordinates) while the user is drawing, or `None` when idle.
    /// The end point is snapped to nearby walls/endpoints.
    pub fn line_segment_preview(&self) -> Option<(Vec2D, Vec2D)> {
        if let LineSegmentDrawState::Drawing { start } = self.line_draw_state {
            let snapped_end = self.snap_world_pos(self.cursor_world_pos);
            Some((start, snapped_end))
        } else {
            None
        }
    }

    /// Returns world-coordinate positions where snap indicators should be
    /// rendered (e.g. small crosshairs at snap targets).
    pub fn snap_indicators(&self) -> Vec<[f32; 2]> {
        let mut indicators = Vec::new();
        if let LineSegmentDrawState::Drawing { start } = self.line_draw_state {
            // Start was snapped at press time — always show indicator there.
            indicators.push([start.x, start.y]);
            // Show indicator at snapped end position if it differs from cursor.
            let snapped_end = self.snap_world_pos(self.cursor_world_pos);
            if snapped_end != self.cursor_world_pos {
                indicators.push([snapped_end.x, snapped_end.y]);
            }
        } else if self.active_tool == OperatorTool::LineSegment {
            // When idle with the line tool selected, show snap indicator at
            // cursor if it would snap.
            let snapped = self.snap_world_pos(self.cursor_world_pos);
            if snapped != self.cursor_world_pos {
                indicators.push([snapped.x, snapped.y]);
            }
        }
        indicators
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects2d::Rect;

    #[test]
    fn line_segment_draw_state_defaults_to_idle() {
        let state = LineSegmentDrawState::default();
        assert!(matches!(state, LineSegmentDrawState::Idle));
    }

    #[test]
    fn line_segment_drawing_holds_start() {
        let state = LineSegmentDrawState::Drawing {
            start: Vec2D::new(1.0, 2.0),
        };
        if let LineSegmentDrawState::Drawing { start } = state {
            assert!((start.x - 1.0).abs() < 1e-6);
            assert!((start.y - 2.0).abs() < 1e-6);
        } else {
            panic!("Expected Drawing state");
        }
    }

    #[test]
    fn moving_endpoint_state_holds_fields() {
        let state = LineSegmentDrawState::MovingEndpoint {
            segment_id: 42,
            move_start: true,
        };
        if let LineSegmentDrawState::MovingEndpoint {
            segment_id,
            move_start,
        } = state
        {
            assert_eq!(segment_id, 42);
            assert!(move_start);
        } else {
            panic!("Expected MovingEndpoint state");
        }
    }

    #[test]
    fn snap_position_snaps_near_wall() {
        use crate::objects2d::line_segment::snap_position;
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let pos = Vec2D::new(-17.7, 0.0);
        let snapped = snap_position(pos, &world, &[], 0.5);
        assert!(
            (snapped.x - (-18.0)).abs() < 1e-6,
            "Should snap to left wall"
        );
        assert!((snapped.y - 0.0).abs() < 1e-6, "y unchanged");
    }

    #[test]
    fn snap_position_snaps_near_endpoint() {
        use crate::objects2d::SimLineSegment;
        use crate::objects2d::line_segment::snap_position;
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let seg = SimLineSegment::new(0, Vec2D::new(5.0, 5.0), Vec2D::new(10.0, 5.0));
        let pos = Vec2D::new(5.2, 5.1);
        let snapped = snap_position(pos, &world, &[seg], 0.5);
        assert!((snapped.x - 5.0).abs() < 1e-6, "Should snap to endpoint x");
        assert!((snapped.y - 5.0).abs() < 1e-6, "Should snap to endpoint y");
    }

    #[test]
    fn snap_position_no_snap_when_far() {
        use crate::objects2d::line_segment::snap_position;
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let pos = Vec2D::new(0.0, 0.0);
        let snapped = snap_position(pos, &world, &[], 0.5);
        assert!((snapped.x - 0.0).abs() < 1e-6, "No snap expected");
        assert!((snapped.y - 0.0).abs() < 1e-6, "No snap expected");
    }

    #[test]
    fn wall_snap_takes_priority_over_endpoint() {
        use crate::objects2d::SimLineSegment;
        use crate::objects2d::line_segment::snap_position;
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let seg = SimLineSegment::new(0, Vec2D::new(-17.9, 0.0), Vec2D::new(0.0, 0.0));
        let pos = Vec2D::new(-17.7, 0.0);
        let snapped = snap_position(pos, &world, &[seg], 0.5);
        assert!(
            (snapped.x - (-18.0)).abs() < 1e-6,
            "Wall snap should take priority"
        );
    }
}
