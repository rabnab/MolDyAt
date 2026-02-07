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
#[allow(unused_imports)] // May be unused depending on renderer feature
pub use energy_window_manager::EnergyWindowManager;
#[allow(unused_imports)] // Unused when building without ggez
pub use event_loop_state::EventLoopState;
#[allow(unused_imports)] // Used by WGPU renderer only
pub use event_loop_state::process_action_common;
pub use input_handler::{InputAction, InputHandler, KeyCode};
pub use metrics::{FrameMetrics, MetricsCollector};
pub use physics_coordinator::PhysicsCoordinator;
#[allow(unused_imports)] // May be used in future refactoring
pub use render_context::RenderContext;
#[allow(unused_imports)]
pub use unified_render_state::UnifiedRenderState;
