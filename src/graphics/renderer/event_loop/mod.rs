mod action_processor;
mod energy_window_manager;
mod input_handler;
pub mod metrics;
mod physics_coordinator;
#[cfg(test)]
mod tests;
mod unified_render_state;

// Shared event loop infrastructure for both ggez and wgpu renderers
#[cfg(feature = "renderer-wgpu")]
pub use action_processor::process_action_common;
#[cfg(feature = "renderer-wgpu")]
pub use energy_window_manager::EnergyWindowManager;
pub use input_handler::{InputAction, InputHandler, KeyCode};
pub use metrics::{FrameMetrics, MetricsCollector};
pub use physics_coordinator::PhysicsCoordinator;
pub use unified_render_state::UnifiedRenderState;
