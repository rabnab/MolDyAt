// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Trait-based operator tool dispatch.
//!
//! Each operator tool (line segment, valve, source sphere, thermostat, …)
//! implements [`OperatorToolHandler`] which receives mouse/scroll events
//! and returns [`ToolResponse`] values.  `UnifiedRenderState` dispatches
//! to the active handler, eliminating per-tool if-chains.

use thermo_physics::objects2d::Vec2D;
use thermo_physics::objects2d::wall_chain::ObjectRef;
use thermo_runtime::application::operator_command::OperatorCommand;
use thermo_runtime::application::renderer_snapshot::OperatorSnapshot;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};

/// Response returned by a tool handler after processing an event.
#[derive(Default)]
pub struct ToolResponse {
    /// Whether the event was consumed (prevents fallthrough to pan/zoom).
    pub consumed: bool,
    /// An operator command to send to the physics thread, if any.
    pub action: Option<Box<dyn OperatorCommand>>,
    /// Whether the view should be redrawn (ghost preview changed, etc.).
    pub view_dirty: bool,
}

impl ToolResponse {
    /// Convenience: event consumed, view dirty, no command.
    pub fn consumed_dirty() -> Self {
        Self {
            consumed: true,
            view_dirty: true,
            ..Default::default()
        }
    }

    /// Convenience: event consumed, view dirty, with a command.
    pub fn consumed_with(action: Box<dyn OperatorCommand>) -> Self {
        Self {
            consumed: true,
            action: Some(action),
            view_dirty: true,
        }
    }
}

/// Preview geometry produced by the active operator tool.
///
/// Uses an enum rather than trait objects to avoid per-frame heap
/// allocation.  The renderer matches on variants to draw ghost outlines.
#[derive(Debug, Clone, Default)]
pub enum ToolPreview {
    /// No preview to render.
    #[default]
    None,
    /// Ghost wall from `start` to `end` (world coordinates), `thickness`
    /// in the same units as `SimLineSegment::thickness`.
    LineSegment {
        start: [f32; 2],
        end: [f32; 2],
        thickness: f32,
    },
    /// Ghost valve overlay on `line_id` from `t_start` to `t_end`.
    ValveSegment {
        line_id: usize,
        t_start: f32,
        t_end: f32,
    },
}

/// Context passed to tool handlers so they can read (but not write)
/// the current simulation state and UI state.
pub struct ToolContext<'a> {
    /// Current operator snapshot (line segments, valves, spheres).
    pub operators: &'a OperatorSnapshot,
    /// World-space bounds of the viewport (for wall snapping).
    pub world_bounds: thermo_physics::objects2d::Rect,
    /// Current cursor position in world coordinates.
    pub cursor_world_pos: Vec2D,
}

/// Trait implemented by each operator tool's interaction handler.
///
/// Implementors own their drawing/interaction state machine and produce
/// [`ToolResponse`] values that the dispatcher forwards to the physics
/// thread.
pub trait OperatorToolHandler: Send {
    /// Handle a mouse button press or release.
    fn handle_mouse_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        world_pos: Vec2D,
        ctx: &ToolContext,
    ) -> ToolResponse;

    /// Handle cursor movement.
    fn handle_cursor_move(&mut self, world_pos: Vec2D, ctx: &ToolContext) -> ToolResponse;

    /// Handle scroll wheel input.
    ///
    /// `scroll_y` is the vertical scroll amount (positive = up).
    /// `ctrl_held` indicates whether Ctrl is pressed.
    fn handle_scroll(
        &mut self,
        scroll_y: f32,
        world_pos: Vec2D,
        ctrl_held: bool,
        ctx: &ToolContext,
    ) -> ToolResponse;

    /// Current ghost-preview geometry, if any.
    fn preview(&self, ctx: &ToolContext) -> ToolPreview;

    /// The object of this tool's own kind under the cursor (or being
    /// dragged), for highlighting and for `Delete`. `None` while drawing a
    /// new object.
    fn hovered(&self, ctx: &ToolContext) -> Option<ObjectRef>;

    /// Snap indicator positions (world coordinates) for crosshair rendering.
    fn snap_indicators(&self, ctx: &ToolContext) -> Vec<[f32; 2]>;

    /// Reset tool state to idle (called on tool deselection or sim clear).
    fn reset(&mut self);

    /// Check if a key is relevant for this tool's scroll modifier behaviour.
    fn uses_key_modifier(&self) -> bool {
        false
    }
}

/// Helper: extract vertical scroll amount from a winit delta.
pub fn scroll_y_from_delta(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 50.0,
    }
}
