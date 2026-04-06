mod action_processor;
mod energy_window_manager;
mod input_handler;
mod line_segment_tool;
pub mod metrics;
mod physics_coordinator;
mod sidebar_tool;
#[cfg(test)]
mod tests;
mod unified_render_state;

// Shared event loop infrastructure for both ggez and wgpu renderers

// EnergyWindowManager is a public field in UnifiedRenderState, must be exported
// even though ggez accesses it only through the field (type needs to be public)
#[allow(unused_imports)]
pub use energy_window_manager::EnergyWindowManager;
pub use input_handler::{InputAction, InputHandler, KeyCode};
pub use metrics::{FrameMetrics, MetricsCollector};
pub use physics_coordinator::PhysicsCoordinator;
pub use unified_render_state::UnifiedRenderState;

// action_processor::process_action_common is now internal - only used by UnifiedRenderState
