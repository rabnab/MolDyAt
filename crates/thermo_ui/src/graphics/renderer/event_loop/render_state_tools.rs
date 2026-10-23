// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The active operator tool of [`UnifiedRenderState`]: selecting it, the
//! context it gets, its previews, and hovering and deleting objects.

use thermo_physics::objects2d::Vec2D;
use thermo_physics::objects2d::wall_chain::ObjectRef;
use thermo_runtime::application::operator_command::delete::DeleteObject;
use thermo_runtime::application::operator_command::line::SetGhostWall;
use thermo_runtime::application::renderer_snapshot::{OperatorSnapshot, RendererSnapshot};

use super::input_handler::OperatorTool;
use super::line_segment_tool::LineSegmentToolHandler;
use super::operator_tool_handler::{OperatorToolHandler, ToolContext, ToolPreview};
use super::source_sphere_tool::SourceSphereToolHandler;
use super::thermostat_tool::ThermostatToolHandler;
use super::valve_segment_tool::ValveSegmentToolHandler;

use super::unified_render_state::{RenderStateData, UnifiedRenderState};
use thermo_runtime::application::DomainCommand;

impl UnifiedRenderState {
    /// Return the active tool to idle, dropping any ghost wall it had sent
    /// to the physics thread.
    pub(super) fn reset_tool_handler(&mut self) {
        if let Some(handler) = self.render.tool_handler.as_mut() {
            handler.reset();
        }
        if self.render.active_tool == OperatorTool::LineSegment {
            self.physics
                .send_command(DomainCommand::Operator(Box::new(SetGhostWall {
                    ghost: None,
                })));
        }
    }

    pub(super) fn select_operator_tool(&mut self, tool: OperatorTool) {
        self.reset_tool_handler();
        self.render.active_tool = tool;
        self.render.tool_handler = Self::handler_for_tool(tool);
        self.render.config.draw_cursor = tool == OperatorTool::Ball;
        self.render.view_dirty = true;
    }

    pub(super) fn current_operator_snapshot(&self) -> &OperatorSnapshot {
        self.render
            .last_snapshot
            .as_deref()
            .map(|snapshot| &snapshot.operators)
            .unwrap_or(&self.render.default_operator_snapshot)
    }

    pub(super) fn can_add_ball_at(&self, position: Vec2D) -> bool {
        !self
            .current_operator_snapshot()
            .is_near_any_operator(position)
    }

    pub(super) fn with_tool_handler<R>(
        &mut self,
        f: impl FnOnce(&mut dyn OperatorToolHandler, ToolContext<'_>) -> R,
    ) -> Option<R> {
        let render = &mut self.render;
        let RenderStateData {
            tool_handler,
            last_snapshot,
            default_operator_snapshot,
            viewport,
            cursor_world_pos,
            ..
        } = render;
        let handler = tool_handler.as_deref_mut()?;
        let ctx = Self::make_tool_context_from(
            last_snapshot.as_deref(),
            default_operator_snapshot,
            viewport,
            *cursor_world_pos,
        );
        Some(f(handler, ctx))
    }

    /// Collect all operator preview geometry for the current frame.
    ///
    /// Delegates to the active [`OperatorToolHandler`] for preview and
    /// snap indicators, then bundles them with the selected sidebar icon.
    pub fn operator_previews(&self) -> super::FramePreviews {
        let ctx = self.make_tool_context();

        let (line_segment, valve_segment, snap_indicators) =
            if let Some(handler) = &self.render.tool_handler {
                let preview = handler.preview(&ctx);
                let snaps = handler.snap_indicators(&ctx);
                match preview {
                    ToolPreview::LineSegment {
                        start,
                        end,
                        thickness,
                    } => (Some((start, end, thickness)), None, snaps),
                    ToolPreview::ValveSegment {
                        line_id,
                        t_start,
                        t_end,
                    } => (None, Some((line_id, t_start, t_end)), snaps),
                    ToolPreview::None => (None, None, snaps),
                }
            } else {
                (None, None, Vec::new())
            };

        super::FramePreviews {
            line_segment,
            valve_segment,
            snap_indicators,
            selected_sidebar_icon: self.selected_sidebar_icon(),
            hovered: self.hovered_object(),
        }
    }

    /// The object the active tool reports under the cursor, if any.
    pub(super) fn hovered_object(&self) -> Option<ObjectRef> {
        let handler = self.render.tool_handler.as_deref()?;
        handler.hovered(&self.make_tool_context())
    }

    /// Delete the hovered object, if any. The tool is reset first so it
    /// can't keep dragging or toggling an object that no longer exists.
    pub(super) fn delete_hovered_object(&mut self) {
        let Some(target) = self.hovered_object() else {
            return;
        };
        self.reset_tool_handler();
        self.physics
            .send_command(DomainCommand::Operator(Box::new(DeleteObject { target })));
        self.render.view_dirty = true;
    }

    /// Build a [`ToolContext`] from the current snapshot.
    ///
    /// The returned context borrows `snapshot` (if `Some`) or the provided
    /// `fallback` default.  Callers that also need `&mut self.tool_handler`
    /// should invoke this with explicit field borrows to avoid conflicting
    /// `&self` / `&mut self` borrows.
    pub(super) fn make_tool_context_from<'a>(
        snapshot: Option<&'a RendererSnapshot>,
        fallback: &'a OperatorSnapshot,
        viewport: &crate::graphics::renderer::ViewportTransform,
        cursor_world_pos: Vec2D,
    ) -> ToolContext<'a> {
        let operators = snapshot.map(|s| &s.operators).unwrap_or(fallback);
        ToolContext {
            operators,
            world_bounds: viewport.world_bounds(),
            cursor_world_pos,
        }
    }

    /// Convenience wrapper when no mutable self-borrow is needed.
    pub(super) fn make_tool_context(&self) -> ToolContext<'_> {
        Self::make_tool_context_from(
            self.render.last_snapshot.as_deref(),
            &self.render.default_operator_snapshot,
            &self.render.viewport,
            self.render.cursor_world_pos,
        )
    }

    /// Create a handler for the given tool, or `None` if no tool selected.
    pub(super) fn handler_for_tool(tool: OperatorTool) -> Option<Box<dyn OperatorToolHandler>> {
        match tool {
            OperatorTool::Ball => None,
            OperatorTool::LineSegment => Some(Box::new(LineSegmentToolHandler::new())),
            OperatorTool::ValveSegment => Some(Box::new(ValveSegmentToolHandler::new())),
            OperatorTool::SourceSphere => Some(Box::new(SourceSphereToolHandler::new())),
            OperatorTool::Thermostat => Some(Box::new(ThermostatToolHandler::new())),
            OperatorTool::None => None,
        }
    }
}
