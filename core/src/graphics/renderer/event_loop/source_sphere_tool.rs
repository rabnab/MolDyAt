//! Source sphere operator tool — placing, toggling, and adjusting source spheres.
//!
//! Defines [`SourceSphereToolHandler`] which implements [`OperatorToolHandler`]
//! for the source-sphere interaction: click to place/toggle, right-click to
//! cycle ball size, scroll to adjust speed/emission period.

use crate::application::operator_command::source::{
    AdjustSourceSphereEmitPeriod, AdjustSourceSphereSpeed, CycleSourceSphereBallSize,
    PlaceSourceSphere, ToggleSourceSphere,
};
use crate::objects2d::wall_chain::ObjectRef;
use crate::objects2d::{EMISSION_SPEED_STEP, SOURCE_SPHERE_RADIUS, Vec2D};
use winit::event::{ElementState, MouseButton};

use super::operator_tool_handler::{OperatorToolHandler, ToolContext, ToolPreview, ToolResponse};

/// How close the cursor must be to a sphere centre (in world units) to
/// register a hit for toggle/interaction.
const SPHERE_HIT_RADIUS: f32 = 1.5;
/// Step size for adjusting emission period via scroll wheel (in simulation steps).
const EMISSION_PERIOD_DELTA: i32 = 500;
/// Default radius for newly placed source spheres (in world units).
pub const DEFAULT_SOURCE_RADIUS: f32 = SOURCE_SPHERE_RADIUS;

/// Self-contained handler for the source sphere operator tool.
pub struct SourceSphereToolHandler;

impl SourceSphereToolHandler {
    pub fn new() -> Self {
        Self
    }

    /// Find a placed source sphere whose centre is within hit radius of `pos`.
    fn find_source_sphere_at(pos: Vec2D, ctx: &ToolContext) -> Option<usize> {
        let threshold_sq = SPHERE_HIT_RADIUS * SPHERE_HIT_RADIUS;
        ctx.operators
            .source_spheres
            .iter()
            .find(|s| pos.distance_squared(&s.center) <= threshold_sq)
            .map(|s| s.id)
    }
}

impl OperatorToolHandler for SourceSphereToolHandler {
    fn handle_mouse_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        world_pos: Vec2D,
        ctx: &ToolContext,
    ) -> ToolResponse {
        match (button, state) {
            // Left release: toggle existing sphere or place new
            (MouseButton::Left, ElementState::Released) => {
                if let Some(sphere_id) = Self::find_source_sphere_at(world_pos, ctx) {
                    ToolResponse::consumed_with(Box::new(ToggleSourceSphere { sphere_id }))
                } else {
                    ToolResponse::consumed_with(Box::new(PlaceSourceSphere { center: world_pos }))
                }
            }
            // Right press on existing sphere: cycle ball size
            (MouseButton::Right, ElementState::Pressed) => {
                if let Some(sphere_id) = Self::find_source_sphere_at(world_pos, ctx) {
                    ToolResponse::consumed_with(Box::new(CycleSourceSphereBallSize { sphere_id }))
                } else {
                    ToolResponse::default()
                }
            }
            _ => ToolResponse::default(),
        }
    }

    fn handle_cursor_move(&mut self, _world_pos: Vec2D, _ctx: &ToolContext) -> ToolResponse {
        ToolResponse::default()
    }

    fn handle_scroll(
        &mut self,
        scroll_y: f32,
        world_pos: Vec2D,
        ctrl_held: bool,
        ctx: &ToolContext,
    ) -> ToolResponse {
        if scroll_y.abs() <= f32::EPSILON {
            return ToolResponse::default();
        }

        let Some(sphere_id) = Self::find_source_sphere_at(world_pos, ctx) else {
            return ToolResponse::default();
        };

        if ctrl_held {
            let delta_steps = if scroll_y > 0.0 {
                // Scroll up → decrease emit period (more frequent)
                -EMISSION_PERIOD_DELTA
            } else {
                // Scroll down → increase emit period (less frequent)
                EMISSION_PERIOD_DELTA
            };
            ToolResponse::consumed_with(Box::new(AdjustSourceSphereEmitPeriod {
                sphere_id,
                delta_steps,
            }))
        } else {
            let speed_delta = if scroll_y > 0.0 {
                EMISSION_SPEED_STEP
            } else {
                -EMISSION_SPEED_STEP
            };
            ToolResponse::consumed_with(Box::new(AdjustSourceSphereSpeed {
                sphere_id,
                delta: speed_delta,
            }))
        }
    }

    fn preview(&self, _ctx: &ToolContext) -> ToolPreview {
        ToolPreview::None
    }

    fn hovered(&self, ctx: &ToolContext) -> Option<ObjectRef> {
        Self::find_source_sphere_at(ctx.cursor_world_pos, ctx).map(ObjectRef::SourceSphere)
    }

    fn snap_indicators(&self, _ctx: &ToolContext) -> Vec<[f32; 2]> {
        Vec::new()
    }

    fn reset(&mut self) {
        // Stateless tool — nothing to reset.
    }

    fn uses_key_modifier(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::renderer_snapshot::OperatorSnapshot;
    use crate::objects2d::{Rect, SimSourceSphere};

    fn make_ctx_data(spheres: Vec<SimSourceSphere>) -> (OperatorSnapshot, Rect) {
        let snap = OperatorSnapshot {
            source_spheres: spheres,
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

    fn test_sphere(id: usize, cx: f32, cy: f32) -> SimSourceSphere {
        SimSourceSphere::new(id, Vec2D::new(cx, cy))
    }

    // ── Placement tests ─────────────────────────────────────────────

    #[test]
    fn left_release_on_empty_places_sphere() {
        let mut tool = SourceSphereToolHandler::new();
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Released,
            Vec2D::new(5.0, 5.0),
            &ctx,
        );
        assert!(resp.consumed);
        assert!(resp.action.is_some(), "Should emit PlaceSourceSphere");
    }

    #[test]
    fn left_release_on_sphere_toggles() {
        let sphere = test_sphere(3, 5.0, 5.0);
        let mut tool = SourceSphereToolHandler::new();
        let (snap, world) = make_ctx_data(vec![sphere]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Released,
            Vec2D::new(5.1, 5.0),
            &ctx,
        );
        assert!(resp.consumed);
        assert!(resp.action.is_some(), "Should emit ToggleSourceSphere");
    }

    #[test]
    fn right_press_on_sphere_cycles_ball_size() {
        let sphere = test_sphere(3, 5.0, 5.0);
        let mut tool = SourceSphereToolHandler::new();
        let (snap, world) = make_ctx_data(vec![sphere]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_mouse_button(
            MouseButton::Right,
            ElementState::Pressed,
            Vec2D::new(5.0, 5.0),
            &ctx,
        );
        assert!(resp.consumed);
        assert!(
            resp.action.is_some(),
            "Should emit CycleSourceSphereBallSize"
        );
    }

    #[test]
    fn right_press_on_empty_ignored() {
        let mut tool = SourceSphereToolHandler::new();
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

    // ── Scroll tests ────────────────────────────────────────────────

    #[test]
    fn scroll_on_sphere_adjusts_speed() {
        let sphere = test_sphere(3, 5.0, 5.0);
        let mut tool = SourceSphereToolHandler::new();
        let (snap, world) = make_ctx_data(vec![sphere]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_scroll(1.0, Vec2D::new(5.0, 5.0), false, &ctx);
        assert!(resp.consumed);
        assert!(resp.action.is_some(), "Should emit AdjustSourceSphereSpeed");
    }

    #[test]
    fn ctrl_scroll_on_sphere_adjusts_emit_period() {
        let sphere = test_sphere(3, 5.0, 5.0);
        let mut tool = SourceSphereToolHandler::new();
        let (snap, world) = make_ctx_data(vec![sphere]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_scroll(1.0, Vec2D::new(5.0, 5.0), true, &ctx);
        assert!(resp.consumed);
        assert!(
            resp.action.is_some(),
            "Should emit AdjustSourceSphereEmitPeriod"
        );
    }

    #[test]
    fn scroll_on_empty_ignored() {
        let mut tool = SourceSphereToolHandler::new();
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_scroll(1.0, Vec2D::new(5.0, 5.0), false, &ctx);
        assert!(!resp.consumed);
    }

    #[test]
    fn zero_scroll_ignored() {
        let sphere = test_sphere(3, 5.0, 5.0);
        let mut tool = SourceSphereToolHandler::new();
        let (snap, world) = make_ctx_data(vec![sphere]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_scroll(0.0, Vec2D::new(5.0, 5.0), false, &ctx);
        assert!(!resp.consumed);
    }

    // ── Preview tests ───────────────────────────────────────────────

    #[test]
    fn preview_always_none() {
        let tool = SourceSphereToolHandler::new();
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());
        assert!(matches!(tool.preview(&ctx), ToolPreview::None));
    }

    #[test]
    fn uses_key_modifier_is_true() {
        let tool = SourceSphereToolHandler::new();
        assert!(tool.uses_key_modifier());
    }
}
