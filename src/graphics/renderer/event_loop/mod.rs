mod energy_window_manager;
mod event_loop_state;
mod input_handler;
pub mod metrics;
mod physics_coordinator;
mod render_context;
#[cfg(test)]
mod tests;
mod unified_render_state;

// Shared event loop infrastructure for both ggez and wgpu renderers
#[cfg(feature = "renderer-wgpu")]
pub use energy_window_manager::EnergyWindowManager;
// EventLoopState is legacy - superseded by UnifiedRenderState
#[allow(unused_imports)]
pub use event_loop_state::EventLoopState;
#[cfg(feature = "renderer-wgpu")]
pub use event_loop_state::process_action_common;
pub use input_handler::{InputAction, InputHandler, KeyCode};
pub use metrics::{FrameMetrics, MetricsCollector};
pub use physics_coordinator::PhysicsCoordinator;
// RenderContext is unused - candidate for removal
#[allow(unused_imports)]
pub use render_context::RenderContext;
pub use unified_render_state::UnifiedRenderState;
