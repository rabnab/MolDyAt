//! Valve segment operator tool — drawing, toggling, and dragging valves on line segments.
//!
//! Defines [`ValveSegmentToolHandler`] which implements [`OperatorToolHandler`]
//! for the valve-segment interaction state machine, hit-testing on existing
//! line segments, and preview queries.

use crate::application::operator_command::valve::{AddValveSegment, MoveValveSegment, ToggleValve};
use crate::objects2d::line_segment::THICKNESS_TO_WORLD;
use crate::objects2d::{SimLineSegment, Vec2D};
use winit::event::{ElementState, MouseButton};

use super::operator_tool_handler::{OperatorToolHandler, ToolContext, ToolPreview, ToolResponse};

/// State machine for valve segment interactions.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ValveDrawState {
    /// Waiting for user interaction.
    #[default]
    Idle,
    /// Drawing a new valve: user pressed on a line segment and is
    /// dragging to define the valve region.
    Drawing { line_id: usize, t_start: f32 },
    /// Dragging an existing valve along its parent segment.
    Moving {
        valve_id: usize,
        line_id: usize,
        /// Half-width of the valve in parametric space, preserved during drag.
        t_half_width: f32,
        /// Parametric center of the valve at press time.
        t_center_at_press: f32,
        /// Parametric position under the cursor at press time. Used to tell a
        /// toggle click from a drag, and to move the valve by the drag distance
        /// (so grabbing away from the centre doesn't make the valve jump).
        t_grab: f32,
    },
}

/// Same padding the renderer uses (move VALVE_PAD_FRAC here from valve_pipeline.rs).
pub const VALVE_PAD_FRAC: f32 = 0.008;

/// Snap threshold for parametric distance on a line segment.
const T_SNAP_THRESHOLD: f32 = 0.05;

/// Maximum perpendicular distance (in world units) from cursor to a line
/// segment for the cursor to be considered "on" that segment.
const LINE_HIT_DISTANCE: f32 = 0.8;

/// Parametric movement threshold to distinguish a toggle click from a drag.
const T_MOVE_THRESHOLD: f32 = 0.03;

/// A valve under the cursor.
struct ValveHit {
    valve_id: usize,
    line_id: usize,
    t_start: f32,
    t_end: f32,
    /// Parametric position along the parent segment under the cursor.
    t_cursor: f32,
}

/// Self-contained handler for the valve segment operator tool.
pub struct ValveSegmentToolHandler {
    pub state: ValveDrawState,
}

impl Default for ValveSegmentToolHandler {
    fn default() -> ValveSegmentToolHandler {
        Self::new()
    }
}

impl ValveSegmentToolHandler {
    pub fn new() -> Self {
        Self {
            state: ValveDrawState::Idle,
        }
    }

    /// Find the line segment closest to `pos` within hit distance
    /// and return `(line_id, t)`.
    fn find_line_at(pos: Vec2D, ctx: &ToolContext) -> Option<(usize, f32)> {
        let mut best_dist = LINE_HIT_DISTANCE;
        let mut best: Option<(usize, f32)> = None;
        for seg in &ctx.operators.line_segments {
            let t = seg.closest_t(&pos);
            let closest = seg.point_at_t(t);
            let half = 0.5 * seg.thickness * THICKNESS_TO_WORLD;
            let dist = pos.dist(&closest).norm() - half;
            if dist < best_dist {
                best_dist = dist;
                best = Some((seg.id, t));
            }
        }
        best
    }

    /// Find a valve whose rectangle contains `pos`.
    ///
    /// The rectangle is the same one the renderer draws (see `valve_half_width`).
    /// If several valves overlap, the one whose centre is closest wins.
    fn find_valve_at(pos: Vec2D, ctx: &ToolContext) -> Option<ValveHit> {
        let world_extent = ctx
            .world_bounds
            .w
            .abs()
            .max(ctx.world_bounds.h.abs())
            .max(1.0);

        let mut best_dist = f32::MAX;
        let mut best: Option<ValveHit> = None;
        for valve in &ctx.operators.valve_segments {
            let Some(seg) = ctx
                .operators
                .line_segments
                .iter()
                .find(|s| s.id == valve.line_id)
            else {
                continue;
            };

            let half_width = Self::valve_half_width(seg, world_extent);
            if !Self::point_in_valve(pos, seg, valve.t_start, valve.t_end, half_width) {
                continue;
            }

            let center = seg.point_at_t((valve.t_start + valve.t_end) * 0.5);
            let dist = pos.dist(&center).norm();
            if dist < best_dist {
                best_dist = dist;
                best = Some(ValveHit {
                    valve_id: valve.id,
                    line_id: valve.line_id,
                    t_start: valve.t_start,
                    t_end: valve.t_end,
                    t_cursor: seg.closest_t(&pos),
                });
            }
        }
        best
    }

    /// Snap parametric coordinate to 0.0 or 1.0 if within threshold.
    fn snap_t(t: f32) -> f32 {
        if t < T_SNAP_THRESHOLD {
            0.0
        } else if t > 1.0 - T_SNAP_THRESHOLD {
            1.0
        } else {
            t
        }
    }

    /// Half-width of a valve rectangle in world units.
    pub fn valve_half_width(seg: &SimLineSegment, world_extent: f32) -> f32 {
        0.5 * seg.thickness * THICKNESS_TO_WORLD + world_extent * VALVE_PAD_FRAC
    }

    /// True if `pos` lies inside the valve rectangle (oriented along the wall).
    fn point_in_valve(
        pos: Vec2D,
        seg: &SimLineSegment,
        t_start: f32,
        t_end: f32,
        half_width: f32,
    ) -> bool {
        let a = seg.point_at_t(t_start);
        let b = seg.point_at_t(t_end);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            return false;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (px, py) = (pos.x - a.x, pos.y - a.y);
        let along = px * ux + py * uy; // distance along the valve from its start
        let across = -px * uy + py * ux; // signed distance from the centre line
        (0.0..=len).contains(&along) && across.abs() <= half_width
    }
}

impl OperatorToolHandler for ValveSegmentToolHandler {
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
                if let Some(hit) = Self::find_valve_at(world_pos, ctx) {
                    self.state = ValveDrawState::Moving {
                        valve_id: hit.valve_id,
                        line_id: hit.line_id,
                        t_half_width: (hit.t_end - hit.t_start) * 0.5,
                        t_center_at_press: (hit.t_start + hit.t_end) * 0.5,
                        t_grab: hit.t_cursor,
                    };
                } else if let Some((line_id, t)) = Self::find_line_at(world_pos, ctx) {
                    let t = Self::snap_t(t);
                    self.state = ValveDrawState::Drawing {
                        line_id,
                        t_start: t,
                    };
                }
                ToolResponse::consumed_dirty()
            }
            ElementState::Released => {
                let response = match self.state {
                    ValveDrawState::Drawing { line_id, t_start } => {
                        if let Some((hit_line, t_end)) = Self::find_line_at(world_pos, ctx)
                            && hit_line == line_id
                        {
                            let t_end = Self::snap_t(t_end);
                            if (t_start - t_end).abs() > 0.02 {
                                ToolResponse::consumed_with(Box::new(AddValveSegment {
                                    line_id,
                                    t_start,
                                    t_end,
                                    is_closed: true,
                                }))
                            } else {
                                ToolResponse::consumed_dirty()
                            }
                        } else {
                            ToolResponse::consumed_dirty()
                        }
                    }
                    ValveDrawState::Moving {
                        valve_id,
                        line_id,
                        t_half_width,
                        t_center_at_press,
                        t_grab,
                    } => {
                        // A drag is measured from where the user grabbed the valve,
                        // not from its centre, so clicking off-centre still toggles.
                        let dragged = Self::find_line_at(world_pos, ctx).filter(|(hit_line, t)| {
                            *hit_line == line_id && (*t - t_grab).abs() > T_MOVE_THRESHOLD
                        });

                        if let Some((_, t_release)) = dragged {
                            // Move by the drag distance so the valve doesn't jump.
                            let t_center = t_center_at_press + (t_release - t_grab);
                            let new_start = (t_center - t_half_width).clamp(0.0, 1.0);
                            let new_end = (t_center + t_half_width).clamp(0.0, 1.0);
                            ToolResponse::consumed_with(Box::new(MoveValveSegment {
                                valve_id,
                                t_start: new_start,
                                t_end: new_end,
                            }))
                        } else {
                            ToolResponse::consumed_with(Box::new(ToggleValve { valve_id }))
                        }
                    }
                    ValveDrawState::Idle => ToolResponse::consumed_dirty(),
                };
                self.state = ValveDrawState::Idle;
                response
            }
        }
    }

    fn handle_cursor_move(&mut self, _world_pos: Vec2D, _ctx: &ToolContext) -> ToolResponse {
        if matches!(self.state, ValveDrawState::Idle) {
            ToolResponse::default()
        } else {
            ToolResponse::consumed_dirty()
        }
    }

    fn handle_scroll(
        &mut self,
        _scroll_y: f32,
        _world_pos: Vec2D,
        _ctrl_held: bool,
        _ctx: &ToolContext,
    ) -> ToolResponse {
        ToolResponse::default()
    }

    fn preview(&self, ctx: &ToolContext) -> ToolPreview {
        if let ValveDrawState::Drawing { line_id, t_start } = self.state {
            if let Some((hit_line, t_end)) = Self::find_line_at(ctx.cursor_world_pos, ctx)
                && hit_line == line_id
            {
                let t_end = Self::snap_t(t_end);
                return ToolPreview::ValveSegment {
                    line_id,
                    t_start,
                    t_end,
                };
            }
            return ToolPreview::ValveSegment {
                line_id,
                t_start,
                t_end: t_start,
            };
        }
        ToolPreview::None
    }

    fn snap_indicators(&self, _ctx: &ToolContext) -> Vec<[f32; 2]> {
        Vec::new()
    }

    fn reset(&mut self) {
        self.state = ValveDrawState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::renderer_snapshot::OperatorSnapshot;
    use crate::objects2d::{Rect, SimLineSegment, ValveSegment};

    fn make_ctx_data(
        segments: Vec<SimLineSegment>,
        valves: Vec<ValveSegment>,
    ) -> (OperatorSnapshot, Rect) {
        let snap = OperatorSnapshot {
            line_segments: segments,
            valve_segments: valves,
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
        let tool = ValveSegmentToolHandler::new();
        assert!(matches!(tool.state, ValveDrawState::Idle));
    }

    #[test]
    fn press_on_line_starts_drawing() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let (snap, world) = make_ctx_data(vec![seg], vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let mut tool = ValveSegmentToolHandler::new();
        let pos = Vec2D::new(5.0, 0.1); // on the line
        let resp = tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, pos, &ctx);
        assert!(resp.consumed);
        assert!(matches!(
            tool.state,
            ValveDrawState::Drawing { line_id: 1, .. }
        ));
    }

    #[test]
    fn press_on_valve_center_starts_moving() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let valve = ValveSegment {
            id: 5,
            line_id: 1,
            t_start: 0.3,
            t_end: 0.7,
            is_closed: true,
        };
        let (snap, world) = make_ctx_data(vec![seg], vec![valve]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let mut tool = ValveSegmentToolHandler::new();
        let pos = Vec2D::new(5.0, 0.0); // center of valve at t=0.5 on seg
        let resp = tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, pos, &ctx);
        assert!(resp.consumed);
        assert!(matches!(
            tool.state,
            ValveDrawState::Moving { valve_id: 5, .. }
        ));
    }

    #[test]
    fn release_after_drawing_commits_valve() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let (snap, world) = make_ctx_data(vec![seg], vec![]);

        let mut tool = ValveSegmentToolHandler::new();

        // Press at t≈0.2
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Pressed,
            Vec2D::new(2.0, 0.1),
            &ctx,
        );

        // Release at t≈0.8
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        let resp = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Released,
            Vec2D::new(8.0, 0.1),
            &ctx,
        );
        assert!(resp.action.is_some(), "Should emit AddValveSegment command");
        assert!(matches!(tool.state, ValveDrawState::Idle));
    }

    #[test]
    fn release_without_drag_on_valve_toggles() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let valve = ValveSegment {
            id: 5,
            line_id: 1,
            t_start: 0.3,
            t_end: 0.7,
            is_closed: true,
        };
        let (snap, world) = make_ctx_data(vec![seg], vec![valve]);

        let mut tool = ValveSegmentToolHandler::new();

        // Press on valve center
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Pressed,
            Vec2D::new(5.0, 0.0),
            &ctx,
        );

        // Release at same position (no drag)
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        let resp = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Released,
            Vec2D::new(5.0, 0.0),
            &ctx,
        );
        assert!(resp.action.is_some(), "Should emit ToggleValve command");
    }

    #[test]
    fn click_off_centre_inside_valve_toggles() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let valve = ValveSegment {
            id: 5,
            line_id: 1,
            t_start: 0.3,
            t_end: 0.7,
            is_closed: true,
        };
        let (snap, world) = make_ctx_data(vec![seg], vec![valve]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let mut tool = ValveSegmentToolHandler::new();
        let pos = Vec2D::new(3.4, 0.1); // near the valve's left end, not its centre
        tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, pos, &ctx);
        assert!(matches!(
            tool.state,
            ValveDrawState::Moving { valve_id: 5, .. }
        ));

        // Release at the same spot: a click, not a drag, so it must toggle.
        let resp = tool.handle_mouse_button(MouseButton::Left, ElementState::Released, pos, &ctx);
        assert!(resp.action.is_some(), "Should emit ToggleValve command");
    }

    #[test]
    fn press_outside_valve_rectangle_does_not_grab_it() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let valve = ValveSegment {
            id: 5,
            line_id: 1,
            t_start: 0.3,
            t_end: 0.7,
            is_closed: true,
        };
        let (snap, world) = make_ctx_data(vec![seg], vec![valve]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let mut tool = ValveSegmentToolHandler::new();
        // Beyond the valve's end along the wall (t = 0.2 < t_start)
        tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Pressed,
            Vec2D::new(2.0, 0.0),
            &ctx,
        );
        assert!(matches!(tool.state, ValveDrawState::Drawing { .. }));
    }

    #[test]
    fn right_click_ignored() {
        let mut tool = ValveSegmentToolHandler::new();
        let (snap, world) = make_ctx_data(vec![], vec![]);
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
        let mut tool = ValveSegmentToolHandler::new();
        tool.state = ValveDrawState::Drawing {
            line_id: 1,
            t_start: 0.3,
        };
        tool.reset();
        assert!(matches!(tool.state, ValveDrawState::Idle));
    }

    // ── Preview tests ───────────────────────────────────────────────

    #[test]
    fn preview_none_when_idle() {
        let tool = ValveSegmentToolHandler::new();
        let (snap, world) = make_ctx_data(vec![], vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::new(5.0, 0.0));
        assert!(matches!(tool.preview(&ctx), ToolPreview::None));
    }

    #[test]
    fn preview_valve_when_drawing() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let (snap, world) = make_ctx_data(vec![seg], vec![]);

        let mut tool = ValveSegmentToolHandler::new();
        tool.state = ValveDrawState::Drawing {
            line_id: 1,
            t_start: 0.2,
        };

        let ctx = make_tool_ctx(&snap, &world, Vec2D::new(7.0, 0.1));
        let preview = tool.preview(&ctx);
        assert!(matches!(
            preview,
            ToolPreview::ValveSegment { line_id: 1, .. }
        ));
    }

    // ── Snap-t tests ────────────────────────────────────────────────

    #[test]
    fn snap_t_snaps_near_zero() {
        assert!((ValveSegmentToolHandler::snap_t(0.03) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn snap_t_snaps_near_one() {
        assert!((ValveSegmentToolHandler::snap_t(0.97) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn snap_t_no_snap_in_middle() {
        let t = 0.5;
        assert!((ValveSegmentToolHandler::snap_t(t) - 0.5).abs() < 1e-6);
    }

    // ── Cursor move redraw ──────────────────────────────────────────

    #[test]
    fn cursor_move_no_redraw_when_idle() {
        let mut tool = ValveSegmentToolHandler::new();
        let (snap, world) = make_ctx_data(vec![], vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        let resp = tool.handle_cursor_move(Vec2D::new(1.0, 1.0), &ctx);
        assert!(!resp.view_dirty);
    }

    #[test]
    fn cursor_move_redraws_when_drawing() {
        let mut tool = ValveSegmentToolHandler::new();
        tool.state = ValveDrawState::Drawing {
            line_id: 1,
            t_start: 0.3,
        };
        let (snap, world) = make_ctx_data(vec![], vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        let resp = tool.handle_cursor_move(Vec2D::new(1.0, 1.0), &ctx);
        assert!(resp.view_dirty);
    }
}
