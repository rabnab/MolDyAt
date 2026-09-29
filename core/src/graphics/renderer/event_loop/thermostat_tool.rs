//! Thermostat operator tool — placement, toggle, drag, and adjustments.

use crate::application::operator_command::thermostat::{
    AdjustThermostatRadius, AdjustThermostatTargetTemperature, MoveThermostat, PlaceThermostat,
    ToggleThermostat,
};
use crate::objects2d::Vec2D;
use crate::objects2d::wall_chain::ObjectRef;
use winit::event::{ElementState, MouseButton};

use super::operator_tool_handler::{OperatorToolHandler, ToolContext, ToolPreview, ToolResponse};

/// How close the cursor must be to a thermostat centre to interact with it.
const THERMOSTAT_HIT_RADIUS: f32 = 1.5;
/// Minimum pointer travel before a press becomes a drag.
const DRAG_THRESHOLD: f32 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DragState {
    Idle,
    Pressed {
        thermostat_id: usize,
        press_pos: Vec2D,
    },
    Dragging {
        thermostat_id: usize,
    },
}

pub struct ThermostatToolHandler {
    drag_state: DragState,
}

impl ThermostatToolHandler {
    pub fn new() -> Self {
        Self {
            drag_state: DragState::Idle,
        }
    }

    fn find_thermostat_at(pos: Vec2D, ctx: &ToolContext) -> Option<usize> {
        let threshold_sq = THERMOSTAT_HIT_RADIUS * THERMOSTAT_HIT_RADIUS;
        ctx.operators
            .thermostats
            .iter()
            .find(|thermostat| pos.distance_squared(&thermostat.position) <= threshold_sq)
            .map(|thermostat| thermostat.id)
    }
}

impl Default for ThermostatToolHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl OperatorToolHandler for ThermostatToolHandler {
    fn handle_mouse_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        world_pos: Vec2D,
        ctx: &ToolContext,
    ) -> ToolResponse {
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => {
                if let Some(thermostat_id) = Self::find_thermostat_at(world_pos, ctx) {
                    self.drag_state = DragState::Pressed {
                        thermostat_id,
                        press_pos: world_pos,
                    };
                    ToolResponse::consumed_dirty()
                } else {
                    ToolResponse::default()
                }
            }
            (MouseButton::Left, ElementState::Released) => {
                let drag_state = self.drag_state;
                self.drag_state = DragState::Idle;

                match drag_state {
                    DragState::Dragging { .. } => ToolResponse::consumed_dirty(),
                    DragState::Pressed { thermostat_id, .. } => {
                        ToolResponse::consumed_with(Box::new(ToggleThermostat { thermostat_id }))
                    }
                    DragState::Idle => ToolResponse::consumed_with(Box::new(PlaceThermostat {
                        position: world_pos,
                    })),
                }
            }
            _ => ToolResponse::default(),
        }
    }

    fn handle_cursor_move(&mut self, world_pos: Vec2D, _ctx: &ToolContext) -> ToolResponse {
        match self.drag_state {
            DragState::Pressed {
                thermostat_id,
                press_pos,
            } => {
                if world_pos.distance_squared(&press_pos) >= DRAG_THRESHOLD * DRAG_THRESHOLD {
                    self.drag_state = DragState::Dragging { thermostat_id };
                    ToolResponse::consumed_with(Box::new(MoveThermostat {
                        thermostat_id,
                        new_pos: world_pos,
                    }))
                } else {
                    ToolResponse::default()
                }
            }
            DragState::Dragging { thermostat_id } => {
                ToolResponse::consumed_with(Box::new(MoveThermostat {
                    thermostat_id,
                    new_pos: world_pos,
                }))
            }
            DragState::Idle => ToolResponse::default(),
        }
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

        let Some(thermostat_id) = Self::find_thermostat_at(world_pos, ctx) else {
            return ToolResponse::default();
        };

        let delta_steps = if scroll_y > 0.0 { 1.0 } else { -1.0 };
        if ctrl_held {
            ToolResponse::consumed_with(Box::new(AdjustThermostatRadius {
                thermostat_id,
                delta_steps,
            }))
        } else {
            ToolResponse::consumed_with(Box::new(AdjustThermostatTargetTemperature {
                thermostat_id,
                delta_steps,
            }))
        }
    }

    fn preview(&self, _ctx: &ToolContext) -> ToolPreview {
        ToolPreview::None
    }

    fn hovered(&self, ctx: &ToolContext) -> Option<ObjectRef> {
        let thermostat_id = match self.drag_state {
            DragState::Idle => Self::find_thermostat_at(ctx.cursor_world_pos, ctx)?,
            DragState::Pressed { thermostat_id, .. } | DragState::Dragging { thermostat_id } => {
                thermostat_id
            }
        };
        Some(ObjectRef::Thermostat(thermostat_id))
    }

    fn snap_indicators(&self, _ctx: &ToolContext) -> Vec<[f32; 2]> {
        Vec::new()
    }

    fn reset(&mut self) {
        self.drag_state = DragState::Idle;
    }

    fn uses_key_modifier(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::renderer_snapshot::OperatorSnapshot;
    use crate::objects2d::{Rect, SimThermostat};

    fn make_ctx_data(thermostats: Vec<SimThermostat>) -> (OperatorSnapshot, Rect) {
        let snap = OperatorSnapshot {
            thermostats,
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

    fn test_thermostat(id: usize, x: f32, y: f32) -> SimThermostat {
        SimThermostat::new(id, Vec2D::new(x, y))
    }

    #[test]
    fn left_release_on_empty_places_thermostat() {
        let mut tool = ThermostatToolHandler::new();
        let (snap, world) = make_ctx_data(vec![]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Released,
            Vec2D::new(5.0, 5.0),
            &ctx,
        );

        assert!(resp.consumed);
        assert!(resp.action.is_some());
    }

    #[test]
    fn click_existing_thermostat_toggles() {
        let mut tool = ThermostatToolHandler::new();
        let (snap, world) = make_ctx_data(vec![test_thermostat(7, 4.0, 4.0)]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let press = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Pressed,
            Vec2D::new(4.0, 4.0),
            &ctx,
        );
        assert!(press.consumed);

        let release = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Released,
            Vec2D::new(4.0, 4.0),
            &ctx,
        );
        assert!(release.consumed);
        assert!(release.action.is_some());
    }

    #[test]
    fn drag_existing_thermostat_emits_move() {
        let mut tool = ThermostatToolHandler::new();
        let (snap, world) = make_ctx_data(vec![test_thermostat(7, 4.0, 4.0)]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let press = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Pressed,
            Vec2D::new(4.0, 4.0),
            &ctx,
        );
        assert!(press.consumed);

        let drag = tool.handle_cursor_move(Vec2D::new(5.0, 4.0), &ctx);
        assert!(drag.consumed);
        assert!(drag.action.is_some());

        let release = tool.handle_mouse_button(
            MouseButton::Left,
            ElementState::Released,
            Vec2D::new(5.0, 4.0),
            &ctx,
        );
        assert!(release.consumed);
        assert!(release.action.is_none());
    }

    #[test]
    fn scroll_adjusts_target_temperature() {
        let mut tool = ThermostatToolHandler::new();
        let (snap, world) = make_ctx_data(vec![test_thermostat(7, 4.0, 4.0)]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_scroll(1.0, Vec2D::new(4.0, 4.0), false, &ctx);
        assert!(resp.consumed);
        assert!(resp.action.is_some());
    }

    #[test]
    fn ctrl_scroll_adjusts_radius() {
        let mut tool = ThermostatToolHandler::new();
        let (snap, world) = make_ctx_data(vec![test_thermostat(7, 4.0, 4.0)]);
        let ctx = make_tool_ctx(&snap, &world, Vec2D::zero());

        let resp = tool.handle_scroll(1.0, Vec2D::new(4.0, 4.0), true, &ctx);
        assert!(resp.consumed);
        assert!(resp.action.is_some());
    }

    #[test]
    fn default_radius_matches_operator_constant() {
        assert!((crate::objects2d::DEFAULT_THERMOSTAT_RADIUS - 5.0).abs() < f32::EPSILON);
    }
}
