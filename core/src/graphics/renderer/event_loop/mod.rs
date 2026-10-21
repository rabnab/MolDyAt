// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod action_processor;
pub mod command_registry;
mod energy_window_manager;
mod frame_orchestrator;
mod input_handler;
mod input_orchestrator;
mod line_segment_tool;
pub mod metrics;
pub mod operator_tool_handler;
mod physics_coordinator;
mod render_state_events;
mod render_state_tools;
mod sidebar_tool;
mod source_sphere_tool;
#[cfg(test)]
mod tests;
mod thermostat_tool;
mod unified_render_state;
pub mod valve_segment_tool;
mod viewport_orchestrator;

// Shared event loop infrastructure for both ggez and wgpu renderers

// EnergyWindowManager is exposed through UnifiedRenderState accessors and must
// remain exported so backend code can name the type.
pub use command_registry::{COMMANDS, CommandDescriptor, CommandKind, UiAction};
pub use energy_window_manager::EnergyWindowManager;
pub use input_handler::{InputAction, InputHandler, KeyCode, OperatorTool};
pub use metrics::{FrameMetrics, MetricsCollector};
pub use operator_tool_handler::{OperatorToolHandler, ToolContext, ToolPreview, ToolResponse};
pub use physics_coordinator::PhysicsCoordinator;
pub use source_sphere_tool::DEFAULT_SOURCE_RADIUS;
pub use thermostat_tool::ThermostatToolHandler;
pub use unified_render_state::UnifiedRenderState;

/// Bundled operator preview geometry for the current frame.
///
/// Produced by [`UnifiedRenderState::operator_previews`] and consumed
/// by `PrepareContext` in the WGPU renderer.  Adding a new tool's
/// preview just extends this struct — no signature changes elsewhere.
#[derive(Debug, Clone, Default)]
pub struct FramePreviews {
    /// Ghost wall while the user draws a wall or drags a wall endpoint:
    /// `(start, end, thickness)` in world coordinates / wall thickness units.
    pub line_segment: Option<([f32; 2], [f32; 2], f32)>,
    /// Ghost-preview valve segment while the user is drawing.
    /// `(line_id, t_start, t_end)` — resolve to world coords via committed segments.
    pub valve_segment: Option<(usize, f32, f32)>,
    /// World-coordinate positions for snap indicators (crosshairs).
    pub snap_indicators: Vec<[f32; 2]>,
    /// Currently selected sidebar icon index, if any.
    pub selected_sidebar_icon: Option<usize>,
    /// Object under the cursor of the active tool's kind, drawn highlighted.
    pub hovered: Option<crate::objects2d::wall_chain::ObjectRef>,
}
