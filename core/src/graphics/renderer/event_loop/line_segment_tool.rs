// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Line segment operator tool — drawing, snapping, and endpoint dragging.
//!
//! Defines [`LineSegmentToolHandler`] which implements [`OperatorToolHandler`]
//! for the line-segment drawing state machine, snap-to-wall / snap-to-endpoint
//! helpers, ghost-preview queries, and the endpoint find/drag interaction.

use crate::application::operator_command::line::{
    AddLineSegment, ChangeLineThickness, MoveLineSegmentEndpoint, SetGhostWall,
};
use crate::objects2d::Vec2D;
use crate::objects2d::line_segment::{
    DEFAULT_THICKNESS, GhostWall, THICKNESS_TO_WORLD, snap_position,
};
use crate::objects2d::wall_chain::ObjectRef;
use winit::event::{ElementState, MouseButton};

use super::operator_tool_handler::{OperatorToolHandler, ToolContext, ToolPreview, ToolResponse};

/// Snap threshold in world units.
const SNAP_THRESHOLD: f32 = 0.5;

/// Squared distance below which a piece endpoint is taken to coincide with a
/// wall endpoint (piece ends are recomputed from the wall, so float noise only).
const WALL_ENDPOINT_MATCH_SQ: f32 = 1e-6;

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
    /// Last ghost wall sent to the physics thread, so an unchanged ghost
    /// (e.g. while the cursor stays snapped) isn't sent again.
    last_sent_ghost: Option<GhostWall>,
}

impl LineSegmentToolHandler {
    pub fn new() -> Self {
        Self {
            state: LineSegmentDrawState::Idle,
            last_sent_ghost: None,
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

    /// True if `p` is one of a wall's own two endpoints, as opposed to an
    /// internal piece boundary next to a valve (not draggable, no marker).
    fn is_wall_endpoint(p: Vec2D, ctx: &ToolContext) -> bool {
        ctx.operators.walls.iter().any(|wall| {
            p.distance_squared(&wall.start) < WALL_ENDPOINT_MATCH_SQ
                || p.distance_squared(&wall.end) < WALL_ENDPOINT_MATCH_SQ
        })
    }

    /// Find a wall endpoint within threshold of `pos`, as `(piece_id, is_start)`.
    ///
    /// Piece boundaries created by valves are skipped: they are not draggable.
    fn find_near_endpoint(pos: Vec2D, ctx: &ToolContext) -> Option<(usize, bool)> {
        let threshold_sq = SNAP_THRESHOLD * SNAP_THRESHOLD;
        let mut best_dist_sq = threshold_sq;
        let mut best: Option<(usize, bool)> = None;
        for seg in &ctx.operators.line_segments {
            let d_start = pos.distance_squared(&seg.start);
            let d_end = pos.distance_squared(&seg.end);
            if d_start < best_dist_sq && Self::is_wall_endpoint(seg.start, ctx) {
                best_dist_sq = d_start;
                best = Some((seg.id, true));
            }
            if d_end < best_dist_sq && Self::is_wall_endpoint(seg.end, ctx) {
                best_dist_sq = d_end;
                best = Some((seg.id, false));
            }
        }
        best
    }

    /// The wall being drawn or dragged, with the cursor at `cursor`: the new
    /// wall while drawing, or the whole dragged wall with its moved endpoint.
    fn ghost_at(&self, cursor: Vec2D, ctx: &ToolContext) -> Option<GhostWall> {
        match self.state {
            LineSegmentDrawState::Idle => None,
            LineSegmentDrawState::Drawing { start } => Some(GhostWall {
                start,
                end: Self::snap_world_pos(cursor, ctx),
                thickness: DEFAULT_THICKNESS,
            }),
            LineSegmentDrawState::MovingEndpoint {
                segment_id,
                move_start,
            } => {
                let wall = ctx.operators.wall_of_piece(segment_id)?;
                let moved = Self::snap_world_pos(cursor, ctx);
                let (start, end) = if move_start {
                    (moved, wall.end)
                } else {
                    (wall.start, moved)
                };
                let thickness = ctx
                    .operators
                    .line_segments
                    .iter()
                    .filter(|seg| wall.piece_ids.contains(&seg.id))
                    .map(|seg| seg.thickness)
                    .reduce(f32::max)
                    .unwrap_or(DEFAULT_THICKNESS);
                Some(GhostWall {
                    start,
                    end,
                    thickness,
                })
            }
        }
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
                            ToolResponse::consumed_with(Box::new(SetGhostWall { ghost: None }))
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
                // Every release path above clears the physics ghost.
                self.last_sent_ghost = None;
                response
            }
        }
    }

    fn handle_cursor_move(&mut self, world_pos: Vec2D, ctx: &ToolContext) -> ToolResponse {
        let ghost = self.ghost_at(world_pos, ctx);
        if ghost.is_some() && ghost != self.last_sent_ghost {
            self.last_sent_ghost = ghost;
            ToolResponse::consumed_with(Box::new(SetGhostWall { ghost }))
        } else {
            ToolResponse::consumed_dirty()
        }
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
        match self.ghost_at(ctx.cursor_world_pos, ctx) {
            Some(g) => ToolPreview::LineSegment {
                start: [g.start.x, g.start.y],
                end: [g.end.x, g.end.y],
                thickness: g.thickness,
            },
            None => ToolPreview::None,
        }
    }

    fn hovered(&self, ctx: &ToolContext) -> Option<ObjectRef> {
        let ops = ctx.operators;
        let pos = ctx.cursor_world_pos;
        let near_endpoint =
            || Self::find_near_endpoint(pos, ctx).and_then(|(id, _)| ops.wall_of_piece(id));
        let wall = match self.state {
            LineSegmentDrawState::Idle => near_endpoint()
                .or_else(|| Self::find_segment_at(pos, ctx).and_then(|id| ops.wall_of_piece(id)))
                .or_else(|| {
                    ops.valve_at(pos, ctx.world_bounds.extent())
                        .and_then(|id| ops.wall_of_valve(id))
                }),
            LineSegmentDrawState::MovingEndpoint { segment_id, .. } => {
                ops.wall_of_piece(segment_id)
            }
            // While drawing, only the wall whose endpoint the new wall would
            // snap to, so its endpoint markers show up as a target.
            LineSegmentDrawState::Drawing { .. } => near_endpoint(),
        };
        wall.map(|w| ObjectRef::Wall(w.chain_id))
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
        self.last_sent_ghost = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::renderer_snapshot::{OperatorSnapshot, WallSnapshot};
    use crate::objects2d::{Rect, SimLineSegment};

    /// A wall with the given id, endpoints and pieces (no valves).
    fn wall(chain_id: usize, start: Vec2D, end: Vec2D, piece_ids: Vec<usize>) -> WallSnapshot {
        WallSnapshot {
            chain_id,
            start,
            end,
            piece_ids,
            valve_ids: Vec::new(),
        }
    }

    /// Every segment is treated as its own unsplit wall, with chain id = segment id.
    fn make_ctx_data(segments: Vec<SimLineSegment>) -> (OperatorSnapshot, Rect) {
        let snap = OperatorSnapshot {
            walls: segments
                .iter()
                .map(|s| wall(s.id, s.start, s.end, vec![s.id]))
                .collect(),
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
        let action = format!("{:?}", resp.action);
        assert!(
            action.contains("SetGhostWall { ghost: None }"),
            "Too-short segment should only clear the ghost, got {action}"
        );
    }

    #[test]
    fn dragging_an_endpoint_sends_and_previews_the_moved_wall() {
        let mut thick = SimLineSegment::new(7, Vec2D::new(3.0, 3.0), Vec2D::new(8.0, 3.0));
        thick.thickness = 4.0;
        let (snap, world) = make_ctx_data(vec![thick]);
        let mut tool = LineSegmentToolHandler::new();
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Pressed,
            Vec2D::new(3.1, 3.1),
            &ctx,
        );

        let cursor = Vec2D::new(3.0, 9.0);
        let resp = tool.handle_cursor_move(cursor, &ctx);
        let action = format!("{:?}", resp.action);
        assert!(action.contains("SetGhostWall"), "got {action}");
        assert!(action.contains("thickness: 4.0"), "got {action}");

        let ctx = make_tool_ctx(&snap, &world, cursor);
        match tool.preview(&ctx) {
            ToolPreview::LineSegment {
                start,
                end,
                thickness,
            } => {
                assert_eq!(start, [3.0, 9.0]);
                assert_eq!(end, [8.0, 3.0]);
                assert_eq!(thickness, 4.0);
            }
            other => panic!("expected a ghost wall, got {other:?}"),
        }
    }

    #[test]
    fn an_unchanged_ghost_is_sent_only_once() {
        let (snap, world) = make_ctx_data(vec![]);
        let mut tool = LineSegmentToolHandler::new();
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Pressed,
            Vec2D::new(1.0, 1.0),
            &ctx,
        );

        let first = tool.handle_cursor_move(Vec2D::new(5.0, 5.0), &ctx);
        let same = tool.handle_cursor_move(Vec2D::new(5.0, 5.0), &ctx);
        let moved = tool.handle_cursor_move(Vec2D::new(6.0, 5.0), &ctx);

        assert!(first.action.is_some());
        assert!(same.action.is_none());
        assert!(same.view_dirty);
        assert!(moved.action.is_some());
    }

    #[test]
    fn idle_cursor_moves_send_no_ghost() {
        let (snap, world) = make_ctx_data(vec![]);
        let mut tool = LineSegmentToolHandler::new();
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_cursor_move(Vec2D::new(1.0, 1.0), &ctx);

        assert!(resp.action.is_none());
        assert!(matches!(tool.preview(&ctx), ToolPreview::None));
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

    #[test]
    fn press_near_internal_piece_boundary_does_not_grab_it() {
        // One wall (0,0)-(10,0) split by a valve: pieces end/start at x=4 / x=6,
        // but the wall's own endpoints are still (0,0) and (10,0).
        let left = SimLineSegment::new(0, Vec2D::new(0.0, 0.0), Vec2D::new(4.0, 0.0));
        let right = SimLineSegment::new(1, Vec2D::new(6.0, 0.0), Vec2D::new(10.0, 0.0));
        let snap = OperatorSnapshot {
            line_segments: vec![left, right],
            walls: vec![wall(
                0,
                Vec2D::new(0.0, 0.0),
                Vec2D::new(10.0, 0.0),
                vec![0, 1],
            )],
            ..Default::default()
        };
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        assert!(LineSegmentToolHandler::find_near_endpoint(Vec2D::new(4.0, 0.1), &ctx).is_none());
        assert!(LineSegmentToolHandler::find_near_endpoint(Vec2D::new(6.0, 0.1), &ctx).is_none());
        assert_eq!(
            LineSegmentToolHandler::find_near_endpoint(Vec2D::new(0.1, 0.0), &ctx),
            Some((0, true))
        );
        assert_eq!(
            LineSegmentToolHandler::find_near_endpoint(Vec2D::new(9.9, 0.0), &ctx),
            Some((1, false))
        );
    }
}
