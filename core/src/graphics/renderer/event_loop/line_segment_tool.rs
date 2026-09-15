//! Line segment operator tool — drawing, snapping, and endpoint dragging.
//!
//! Defines [`LineSegmentToolHandler`] which implements [`OperatorToolHandler`]
//! for the line-segment drawing state machine, snap-to-wall / snap-to-endpoint
//! helpers, ghost-preview queries, and the endpoint find/drag interaction.

use crate::application::operator_command::line::{
    AddLineSegment, ChangeLineThickness, MoveLineSegmentEndpoint,
};
use crate::objects2d::Vec2D;
use crate::objects2d::line_segment::{THICKNESS_TO_WORLD, snap_position};
use winit::event::{ElementState, MouseButton};

use super::operator_tool_handler::{OperatorToolHandler, ToolContext, ToolPreview, ToolResponse};

/// Snap threshold in world units.
const SNAP_THRESHOLD: f32 = 0.5;

/// Extra reach (world units) around a wall's surface that still counts as hovering it.
const HOVER_MARGIN: f32 = 0.2;

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

/// Self-contained handler for the line segment operator tool.
pub struct LineSegmentToolHandler {
    pub state: LineSegmentDrawState,
}

impl LineSegmentToolHandler {
    pub fn new() -> Self {
        Self {
            state: LineSegmentDrawState::Idle,
        }
    }

    /// Snap a world position to nearby walls or existing segment endpoints.
    fn snap_world_pos(pos: Vec2D, ctx: &ToolContext) -> Vec2D {
        snap_position(
            pos,
            &ctx.world_bounds,
            &ctx.operators.line_segments,
            SNAP_THRESHOLD,
        )
    }

    /// Find a segment whose endpoint is within threshold of `pos`.
    fn find_near_endpoint(pos: Vec2D, ctx: &ToolContext) -> Option<(usize, bool)> {
        let threshold_sq = SNAP_THRESHOLD * SNAP_THRESHOLD;
        let mut best_dist_sq = threshold_sq;
        let mut best: Option<(usize, bool)> = None;
        for seg in &ctx.operators.line_segments {
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

    /// Squared distance from point `p` to the segment `a`–`b`.
    fn point_segment_dist_sq(p: Vec2D, a: Vec2D, b: Vec2D) -> f32 {
        let (abx, aby) = (b.x - a.x, b.y - a.y);
        let len_sq = abx * abx + aby * aby;
        let t = if len_sq > f32::EPSILON {
            (((p.x - a.x) * abx + (p.y - a.y) * aby) / len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (dx, dy) = (p.x - (a.x + t * abx), p.y - (a.y + t * aby));
        dx * dx + dy * dy
    }

    /// Find the segment whose body is closest to `pos`, within HOVER_THRESHOLD.
    fn find_segment_at(pos: Vec2D, ctx: &ToolContext) -> Option<usize> {
        let mut best_gap = HOVER_MARGIN;
        let mut best = None;
        for seg in &ctx.operators.line_segments {
            let half = 0.5 * seg.thickness * THICKNESS_TO_WORLD;
            let gap = Self::point_segment_dist_sq(pos, seg.start, seg.end).sqrt() - half;
            if gap < best_gap {
                best_gap = gap;
                best = Some(seg.id);
            }
        }
        best
    }
}

impl OperatorToolHandler for LineSegmentToolHandler {
    fn handle_mouse_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        world_pos: Vec2D,
        ctx: &ToolContext,
    ) -> ToolResponse {
        if button != MouseButton::Left {
            return ToolResponse::default();
        }

        match state {
            ElementState::Pressed => {
                if let Some((seg_id, move_start)) = Self::find_near_endpoint(world_pos, ctx) {
                    self.state = LineSegmentDrawState::MovingEndpoint {
                        segment_id: seg_id,
                        move_start,
                    };
                } else {
                    let snapped = Self::snap_world_pos(world_pos, ctx);
                    self.state = LineSegmentDrawState::Drawing { start: snapped };
                }
                ToolResponse::consumed_dirty()
            }
            ElementState::Released => {
                let response = match self.state {
                    LineSegmentDrawState::Drawing { start } => {
                        let end = Self::snap_world_pos(world_pos, ctx);
                        if start.distance_squared(&end) > 0.01 {
                            ToolResponse::consumed_with(Box::new(AddLineSegment { start, end }))
                        } else {
                            ToolResponse::consumed_dirty()
                        }
                    }
                    LineSegmentDrawState::MovingEndpoint {
                        segment_id,
                        move_start,
                    } => {
                        let snapped = Self::snap_world_pos(world_pos, ctx);
                        ToolResponse::consumed_with(Box::new(MoveLineSegmentEndpoint {
                            segment_id,
                            new_pos: snapped,
                            move_start,
                        }))
                    }
                    LineSegmentDrawState::Idle => ToolResponse::consumed_dirty(),
                };
                self.state = LineSegmentDrawState::Idle;
                response
            }
        }
    }

    fn handle_cursor_move(&mut self, _world_pos: Vec2D, _ctx: &ToolContext) -> ToolResponse {
        ToolResponse::consumed_dirty()
    }

    fn handle_scroll(
        &mut self,
        scroll_y: f32,
        world_pos: Vec2D,
        ctrl_held: bool,
        ctx: &ToolContext,
    ) -> ToolResponse {
        // Only while idle, so scrolling can't change a wall mid-draw or mid-drag.
        if !ctrl_held || scroll_y == 0.0 || !matches!(self.state, LineSegmentDrawState::Idle) {
            return ToolResponse::default();
        }

        match Self::find_segment_at(world_pos, ctx) {
            Some(segment_id) => ToolResponse::consumed_with(Box::new(ChangeLineThickness {
                segment_id,
                steps: scroll_y.signum() as i32, // wheel up = +1, down = -1
            })),
            // Not over a wall: leave the event unconsumed so Ctrl+scroll can still do
            // whatever it does elsewhere (e.g. zoom).
            None => ToolResponse::default(),
        }
    }

    fn preview(&self, ctx: &ToolContext) -> ToolPreview {
        if let LineSegmentDrawState::Drawing { start } = self.state {
            let snapped_end = Self::snap_world_pos(ctx.cursor_world_pos, ctx);
            ToolPreview::LineSegment {
                start: [start.x, start.y],
                end: [snapped_end.x, snapped_end.y],
            }
        } else {
            ToolPreview::None
        }
    }

    fn snap_indicators(&self, ctx: &ToolContext) -> Vec<[f32; 2]> {
        let mut indicators = Vec::new();
        if let LineSegmentDrawState::Drawing { start } = self.state {
            indicators.push([start.x, start.y]);
            let snapped_end = Self::snap_world_pos(ctx.cursor_world_pos, ctx);
            if snapped_end != ctx.cursor_world_pos {
                indicators.push([snapped_end.x, snapped_end.y]);
            }
        } else {
            let snapped = Self::snap_world_pos(ctx.cursor_world_pos, ctx);
            if snapped != ctx.cursor_world_pos {
                indicators.push([snapped.x, snapped.y]);
            }
        }
        indicators
    }

    fn reset(&mut self) {
        self.state = LineSegmentDrawState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::renderer_snapshot::OperatorSnapshot;
    use crate::objects2d::{Rect, SimLineSegment};

    fn make_ctx_data(segments: Vec<SimLineSegment>) -> (OperatorSnapshot, Rect) {
        let snap = OperatorSnapshot {
            line_segments: segments,
            ..Default::default()
        };
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        (snap, world)
    }

    fn make_tool_ctx<'a>(
        snap: &'a OperatorSnapshot,
        world: &'a Rect,
        cursor: Vec2D,
    ) -> ToolContext<'a> {
        ToolContext {
            operators: snap,
            world_bounds: *world,
            cursor_world_pos: cursor,
        }
    }

    // ── State machine tests ─────────────────────────────────────────

    #[test]
    fn defaults_to_idle() {
        let tool = LineSegmentToolHandler::new();
        assert!(matches!(tool.state, LineSegmentDrawState::Idle));
    }

    #[test]
    fn left_press_starts_drawing() {
        let mut tool = LineSegmentToolHandler::new();
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        let pos = Vec2D::new(5.0, 5.0);

        let resp = tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, pos, &ctx);
        assert!(resp.consumed);
        assert!(resp.view_dirty);
        assert!(matches!(tool.state, LineSegmentDrawState::Drawing { .. }));
    }

    #[test]
    fn left_press_near_endpoint_starts_moving() {
        let seg = SimLineSegment::new(7, Vec2D::new(3.0, 3.0), Vec2D::new(8.0, 3.0));
        let (snap, world) = make_ctx_data(vec![seg]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let mut tool = LineSegmentToolHandler::new();
        let pos = Vec2D::new(3.1, 3.1);
        let resp = tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, pos, &ctx);
        assert!(resp.consumed);
        assert!(matches!(
            tool.state,
            LineSegmentDrawState::MovingEndpoint {
                segment_id: 7,
                move_start: true
            }
        ));
    }

    #[test]
    fn release_after_drawing_commits_segment() {
        let mut tool = LineSegmentToolHandler::new();
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Pressed,
            Vec2D::new(2.0, 2.0),
            &ctx,
        );

        let resp = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Released,
            Vec2D::new(10.0, 10.0),
            &ctx,
        );
        assert!(resp.consumed);
        assert!(resp.action.is_some(), "Should emit AddLineSegment command");
        assert!(matches!(tool.state, LineSegmentDrawState::Idle));
    }

    #[test]
    fn release_at_same_point_does_not_commit() {
        let mut tool = LineSegmentToolHandler::new();
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let pos = Vec2D::new(5.0, 5.0);
        tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, pos, &ctx);
        let resp = tool.handle_mouse_button(MouseButton::Left, ElementState::Released, pos, &ctx);
        assert!(resp.consumed);
        assert!(resp.action.is_none(), "Too-short segment should not commit");
    }

    #[test]
    fn release_after_moving_endpoint_commits() {
        let seg = SimLineSegment::new(3, Vec2D::new(1.0, 1.0), Vec2D::new(6.0, 1.0));
        let (snap, world) = make_ctx_data(vec![seg]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let mut tool = LineSegmentToolHandler::new();
        tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Pressed,
            Vec2D::new(1.1, 1.0),
            &ctx,
        );
        assert!(matches!(
            tool.state,
            LineSegmentDrawState::MovingEndpoint { .. }
        ));

        let resp = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Released,
            Vec2D::new(2.0, 3.0),
            &ctx,
        );
        assert!(resp.action.is_some(), "Should emit MoveLineSegmentEndpoint");
        assert!(matches!(tool.state, LineSegmentDrawState::Idle));
    }

    #[test]
    fn right_click_ignored() {
        let mut tool = LineSegmentToolHandler::new();
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_mouse_button(
            MouseButton::Right,
            ElementState::Pressed,
            Vec2D::new(5.0, 5.0),
            &ctx,
        );
        assert!(!resp.consumed);
    }

    #[test]
    fn reset_returns_to_idle() {
        let mut tool = LineSegmentToolHandler::new();
        tool.state = LineSegmentDrawState::Drawing {
            start: Vec2D::new(1.0, 1.0),
        };
        tool.reset();
        assert!(matches!(tool.state, LineSegmentDrawState::Idle));
    }

    // ── Preview tests ───────────────────────────────────────────────

    #[test]
    fn preview_none_when_idle() {
        let tool = LineSegmentToolHandler::new();
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::new(5.0, 5.0));
        assert!(matches!(tool.preview(&ctx), ToolPreview::None));
    }

    #[test]
    fn preview_line_when_drawing() {
        let mut tool = LineSegmentToolHandler::new();
        tool.state = LineSegmentDrawState::Drawing {
            start: Vec2D::new(1.0, 2.0),
        };
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::new(8.0, 8.0));
        let preview = tool.preview(&ctx);
        assert!(matches!(preview, ToolPreview::LineSegment { .. }));
    }

    // ── Snap tests ──────────────────────────────────────────────────

    #[test]
    fn snap_position_snaps_near_wall() {
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
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let seg = SimLineSegment::new(0, Vec2D::new(5.0, 5.0), Vec2D::new(10.0, 5.0));
        let pos = Vec2D::new(5.2, 5.1);
        let snapped = snap_position(pos, &world, &[seg], 0.5);
        assert!((snapped.x - 5.0).abs() < 1e-6, "Should snap to endpoint x");
        assert!((snapped.y - 5.0).abs() < 1e-6, "Should snap to endpoint y");
    }

    #[test]
    fn snap_position_no_snap_when_far() {
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let pos = Vec2D::new(0.0, 0.0);
        let snapped = snap_position(pos, &world, &[], 0.5);
        assert!((snapped.x - 0.0).abs() < 1e-6, "No snap expected");
        assert!((snapped.y - 0.0).abs() < 1e-6, "No snap expected");
    }

    #[test]
    fn wall_snap_takes_priority_over_endpoint() {
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let seg = SimLineSegment::new(0, Vec2D::new(-17.9, 0.0), Vec2D::new(0.0, 0.0));
        let pos = Vec2D::new(-17.7, 0.0);
        let snapped = snap_position(pos, &world, &[seg], 0.5);
        assert!(
            (snapped.x - (-18.0)).abs() < 1e-6,
            "Wall snap should take priority"
        );
    }

    // ── Snap indicator tests ────────────────────────────────────────

    #[test]
    fn snap_indicators_empty_when_idle_no_snap() {
        let tool = LineSegmentToolHandler::new();
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::new(5.0, 5.0));
        assert!(tool.snap_indicators(&ctx).is_empty());
    }

    #[test]
    fn snap_indicators_show_start_when_drawing() {
        let mut tool = LineSegmentToolHandler::new();
        tool.state = LineSegmentDrawState::Drawing {
            start: Vec2D::new(1.0, 2.0),
        };
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::new(5.0, 5.0));
        let indicators = tool.snap_indicators(&ctx);
        assert!(!indicators.is_empty(), "Should show start indicator");
        assert!((indicators[0][0] - 1.0).abs() < 1e-6);
        assert!((indicators[0][1] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn ctrl_scroll_over_segment_emits_thickness_command() {
        let seg = SimLineSegment::new(4, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let (snap, world) = make_ctx_data(vec![seg]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        let mut tool = LineSegmentToolHandler::new();

        let resp = tool.handle_scroll(1.0, Vec2D::new(5.0, 0.2), true, &ctx);
        assert!(resp.consumed);
        assert!(resp.action.is_some());
    }

    #[test]
    fn scroll_without_ctrl_or_away_from_segment_is_ignored() {
        let seg = SimLineSegment::new(4, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let (snap, world) = make_ctx_data(vec![seg]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        let mut tool = LineSegmentToolHandler::new();

        let no_ctrl = tool.handle_scroll(1.0, Vec2D::new(5.0, 0.2), false, &ctx);
        assert!(!no_ctrl.consumed);

        let far_away = tool.handle_scroll(1.0, Vec2D::new(5.0, 5.0), true, &ctx);
        assert!(!far_away.consumed);
    }
}
