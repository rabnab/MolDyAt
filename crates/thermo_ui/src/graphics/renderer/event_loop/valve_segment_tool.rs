// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Valve segment operator tool — drawing, toggling, moving and resizing
//! valves on walls.
//!
//! Defines [`ValveSegmentToolHandler`] which implements [`OperatorToolHandler`]
//! for the valve-segment interaction state machine, hit-testing on existing
//! line segments, and preview queries. A click on a valve toggles it; a drag
//! from its middle moves it along its wall, a drag from one of its ends
//! moves that end (#203). The physics thread keeps the valve between its
//! neighbouring pieces (`SetValveSpan`).

use thermo_physics::objects2d::Vec2D;
use thermo_physics::objects2d::line_segment::THICKNESS_TO_WORLD;
use thermo_physics::objects2d::wall_chain::ObjectRef;
use thermo_runtime::application::operator_command::valve::{
    AddValveSegment, SetValveSpan, ToggleValve,
};
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
    /// Pressed on an existing valve: a click toggles it on release, a drag
    /// moves or resizes it ([`ValveGrab`]).
    Pressed {
        valve_id: usize,
        press_pos: Vec2D,
        /// `None` for a valve the snapshot has no wall for: it can only be
        /// toggled.
        grab: Option<ValveGrab>,
        /// The cursor left the click slop: this is a drag, not a click.
        dragging: bool,
    },
}

/// Which part of a valve a press grabbed, and where the valve and its wall
/// were then, so every drag step targets an absolute span.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValveGrab {
    pub grip: ValveGrip,
    /// The wall's (chain's) own endpoints.
    pub chain_start: Vec2D,
    pub chain_end: Vec2D,
    /// The valve's span at the press, in chain fractions.
    pub t_start: f32,
    pub t_end: f32,
    /// The chain fraction of the press.
    pub t_press: f32,
}

/// A valve's grabbed part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValveGrip {
    /// The middle: the whole valve moves.
    Middle,
    /// The end nearer the chain's start: only that end moves.
    Start,
    /// The end nearer the chain's end.
    End,
}

/// Part of a valve's length at each end that grabs that end (resize); the
/// rest moves the whole valve.
const END_GRIP_FRAC: f32 = 0.25;

impl ValveGrab {
    /// The grab of a press at `pos` on valve `valve_id`, from the snapshot;
    /// `None` for a valve without a wall or a wall without length.
    fn at(valve_id: usize, pos: Vec2D, ctx: &ToolContext) -> Option<Self> {
        let wall = ctx.operators.wall_of_valve(valve_id)?;
        let span = ctx
            .operators
            .valve_spans
            .iter()
            .find(|v| v.valve_id == valve_id)?;
        let (a, b) = (wall.start, wall.end);
        let fraction = |p: Vec2D| chain_fraction(a, b, p);
        let (t_start, t_end, t_press) = (fraction(span.p0)?, fraction(span.p1)?, fraction(pos)?);
        let width = (t_end - t_start).max(f32::EPSILON);
        let along = (t_press - t_start) / width;
        let grip = if along < END_GRIP_FRAC {
            ValveGrip::Start
        } else if along > 1.0 - END_GRIP_FRAC {
            ValveGrip::End
        } else {
            ValveGrip::Middle
        };
        Some(Self {
            grip,
            chain_start: a,
            chain_end: b,
            t_start,
            t_end,
            t_press,
        })
    }

    /// The span `(t_start, t_end)` asked for with the cursor at `pos`: the
    /// grabbed part follows the cursor along the wall.
    pub fn target(&self, pos: Vec2D) -> Option<(f32, f32)> {
        let delta = chain_fraction(self.chain_start, self.chain_end, pos)? - self.t_press;
        Some(match self.grip {
            ValveGrip::Middle => (self.t_start + delta, self.t_end + delta),
            ValveGrip::Start => (self.t_start + delta, self.t_end),
            ValveGrip::End => (self.t_start, self.t_end + delta),
        })
    }
}

/// The fraction of `p`'s projection along `a..b` (not clamped); `None` for a
/// wall without length.
fn chain_fraction(a: Vec2D, b: Vec2D, p: Vec2D) -> Option<f32> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length_sq = dx * dx + dy * dy;
    (length_sq > f32::EPSILON).then(|| ((p.x - a.x) * dx + (p.y - a.y) * dy) / length_sq)
}

/// Snap threshold for parametric distance on a line segment.
const T_SNAP_THRESHOLD: f32 = 0.05;

/// Maximum perpendicular distance (in world units) from cursor to a line
/// segment for the cursor to be considered "on" that segment.
const LINE_HIT_DISTANCE: f32 = 0.8;

/// Cursor travel (fraction of the world extent) beyond which a press on a
/// valve counts as a drag instead of a click.
const CLICK_SLOP_FRAC: f32 = 0.005;

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

    /// Find a valve whose rectangle (as drawn) contains `pos`.
    fn find_valve_at(pos: Vec2D, ctx: &ToolContext) -> Option<usize> {
        ctx.operators.valve_at(pos, ctx.world_bounds.extent())
    }

    /// Whether the cursor at `pos` is farther than the click slop from the
    /// press at `press_pos`.
    fn beyond_click_slop(press_pos: Vec2D, pos: Vec2D, ctx: &ToolContext) -> bool {
        press_pos.dist(&pos).norm() > ctx.world_bounds.extent() * CLICK_SLOP_FRAC
    }

    /// The command that moves the grabbed valve with the cursor at `pos`
    /// (none without a grab: a valve the snapshot has no wall for).
    fn drag_to(valve_id: usize, grab: Option<&ValveGrab>, pos: Vec2D) -> ToolResponse {
        match grab.and_then(|grab| grab.target(pos)) {
            Some((t_start, t_end)) => ToolResponse::consumed_with(Box::new(SetValveSpan {
                valve_id,
                t_start,
                t_end,
            })),
            None => ToolResponse::consumed_dirty(),
        }
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
                if let Some(valve_id) = Self::find_valve_at(world_pos, ctx) {
                    self.state = ValveDrawState::Pressed {
                        valve_id,
                        press_pos: world_pos,
                        grab: ValveGrab::at(valve_id, world_pos, ctx),
                        dragging: false,
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
                    ValveDrawState::Pressed {
                        valve_id,
                        press_pos,
                        grab,
                        dragging,
                    } => {
                        if dragging || Self::beyond_click_slop(press_pos, world_pos, ctx) {
                            // The final position of a drag.
                            Self::drag_to(valve_id, grab.as_ref(), world_pos)
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

    fn handle_cursor_move(&mut self, world_pos: Vec2D, ctx: &ToolContext) -> ToolResponse {
        match &mut self.state {
            ValveDrawState::Idle => ToolResponse::default(),
            ValveDrawState::Drawing { .. } => ToolResponse::consumed_dirty(),
            ValveDrawState::Pressed {
                valve_id,
                press_pos,
                grab,
                dragging,
            } => {
                *dragging |= Self::beyond_click_slop(*press_pos, world_pos, ctx);
                if *dragging {
                    Self::drag_to(*valve_id, grab.as_ref(), world_pos)
                } else {
                    ToolResponse::consumed_dirty()
                }
            }
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

    fn hovered(&self, ctx: &ToolContext) -> Option<ObjectRef> {
        let valve_id = match self.state {
            ValveDrawState::Idle => Self::find_valve_at(ctx.cursor_world_pos, ctx)?,
            ValveDrawState::Pressed { valve_id, .. } => valve_id,
            ValveDrawState::Drawing { .. } => return None,
        };
        Some(ObjectRef::Valve(valve_id))
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
    use thermo_physics::objects2d::{Rect, SimLineSegment};
    use thermo_runtime::application::renderer_snapshot::{
        OperatorSnapshot, ValveSpan, WallSnapshot,
    };

    /// The operators of `segments` and `valves`; every valve is cut into
    /// one wall from (0, 0) to (10, 0).
    fn make_ctx_data(
        segments: Vec<SimLineSegment>,
        valves: Vec<ValveSpan>,
    ) -> (OperatorSnapshot, Rect) {
        let walls = vec![WallSnapshot {
            chain_id: 0,
            start: Vec2D::new(0.0, 0.0),
            end: Vec2D::new(10.0, 0.0),
            piece_ids: segments.iter().map(|s| s.id).collect(),
            valve_ids: valves.iter().map(|v| v.valve_id).collect(),
        }];
        let snap = OperatorSnapshot {
            line_segments: segments,
            valve_spans: valves,
            walls,
            ..Default::default()
        };
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        (snap, world)
    }

    /// A closed, thickness-1 valve on the x axis from `x0` to `x1`.
    fn valve_span(valve_id: usize, x0: f32, x1: f32) -> ValveSpan {
        ValveSpan {
            valve_id,
            p0: Vec2D::new(x0, 0.0),
            p1: Vec2D::new(x1, 0.0),
            thickness: 1.0,
            is_closed: true,
        }
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
    fn press_on_valve_center_starts_pressed() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let valve = valve_span(5, 3.0, 7.0);
        let (snap, world) = make_ctx_data(vec![seg], vec![valve]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let mut tool = ValveSegmentToolHandler::new();
        let pos = Vec2D::new(5.0, 0.0); // center of valve at t=0.5 on seg
        let resp = tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, pos, &ctx);
        assert!(resp.consumed);
        assert!(matches!(
            tool.state,
            ValveDrawState::Pressed { valve_id: 5, .. }
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
        let valve = valve_span(5, 3.0, 7.0);
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
        let valve = valve_span(5, 3.0, 7.0);
        let (snap, world) = make_ctx_data(vec![seg], vec![valve]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let mut tool = ValveSegmentToolHandler::new();
        let pos = Vec2D::new(3.4, 0.1); // near the valve's left end, not its centre
        tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, pos, &ctx);
        assert!(matches!(
            tool.state,
            ValveDrawState::Pressed { valve_id: 5, .. }
        ));

        // Release at the same spot: a click, not a drag, so it must toggle.
        let resp = tool.handle_mouse_button(MouseButton::Left, ElementState::Released, pos, &ctx);
        assert!(resp.action.is_some(), "Should emit ToggleValve command");
    }

    /// Press on the valve 3..7 at `from`, drag to `to`, release there: the
    /// span the last command asks for, or `None` without a command.
    fn drag_valve(from: f32, to: f32) -> Option<(f32, f32)> {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let (snap, world) = make_ctx_data(vec![seg], vec![valve_span(5, 3.0, 7.0)]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let mut tool = ValveSegmentToolHandler::new();
        let press = Vec2D::new(from, 0.0);
        tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, press, &ctx);
        let moved = tool.handle_cursor_move(Vec2D::new(to, 0.2), &ctx);
        assert!(moved.consumed);
        let resp = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Released,
            Vec2D::new(to, 0.2),
            &ctx,
        );
        assert!(matches!(tool.state, ValveDrawState::Idle));
        let action = resp.action?;
        let text = format!("{action:?}");
        assert!(text.starts_with("SetValveSpan"), "{text}");
        let span = |name: &str| -> f32 {
            let rest = &text[text.find(name).unwrap() + name.len() + 2..];
            rest.split([',', ' ', '}']).next().unwrap().parse().unwrap()
        };
        Some((span("t_start"), span("t_end")))
    }

    fn assert_span(actual: Option<(f32, f32)>, expected: (f32, f32)) {
        let (s, e) = actual.expect("a valve command");
        assert!(
            (s - expected.0).abs() < 1e-5 && (e - expected.1).abs() < 1e-5,
            "{s}..{e}, expected {expected:?}"
        );
    }

    #[test]
    fn dragging_a_valve_by_its_middle_moves_it_along_the_wall() {
        // Valve at fractions 0.3..0.7 of the wall; the cursor moves by 0.15.
        assert_span(drag_valve(5.0, 6.5), (0.45, 0.85));
    }

    #[test]
    fn dragging_a_valve_end_resizes_it() {
        assert_span(drag_valve(3.4, 2.0), (0.16, 0.7));
        assert_span(drag_valve(6.8, 8.0), (0.3, 0.82));
    }

    #[test]
    fn a_click_still_toggles_and_a_drag_does_not() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let (snap, world) = make_ctx_data(vec![seg], vec![valve_span(5, 3.0, 7.0)]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        let mut tool = ValveSegmentToolHandler::new();
        let at = Vec2D::new(5.0, 0.0);
        tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, at, &ctx);
        // A jitter inside the click slop is no drag.
        let jitter = tool.handle_cursor_move(Vec2D::new(5.05, 0.0), &ctx);
        assert!(jitter.action.is_none());
        let resp = tool.handle_mouse_button(MouseButton::Left, ElementState::Released, at, &ctx);
        assert!(format!("{:?}", resp.action.unwrap()).starts_with("ToggleValve"));
    }

    #[test]
    fn every_drag_step_moves_the_valve_live() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let (snap, world) = make_ctx_data(vec![seg], vec![valve_span(5, 3.0, 7.0)]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        let mut tool = ValveSegmentToolHandler::new();
        let at = Vec2D::new(5.0, 0.0);
        tool.handle_mouse_button(MouseButton::Left, ElementState::Pressed, at, &ctx);
        for x in [5.5, 6.0, 6.5] {
            let step = tool.handle_cursor_move(Vec2D::new(x, 0.0), &ctx);
            assert!(step.action.is_some(), "x = {x}");
        }
        assert!(matches!(
            tool.state,
            ValveDrawState::Pressed { dragging: true, .. }
        ));
    }

    #[test]
    fn press_outside_valve_rectangle_does_not_grab_it() {
        let seg = SimLineSegment::new(1, Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
        let valve = valve_span(5, 3.0, 7.0);
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
