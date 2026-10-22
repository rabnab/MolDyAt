// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Vec2D;
use crate::objects2d::line_segment::snap_position;
use winit::event::{ElementState, MouseButton};

use crate::application::renderer_snapshot::{OperatorSnapshot, WallSnapshot};
use crate::graphics::renderer::event_loop::line_segment_tool::*;
use crate::graphics::renderer::event_loop::operator_tool_handler::{
    OperatorToolHandler, ToolContext, ToolPreview,
};
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
