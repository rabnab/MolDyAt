mod energy_window_manager;
mod event_loop_state;
mod input_handler;
mod physics_coordinator;
mod render_context;
#[cfg(test)]
mod tests;

// Shared event loop infrastructure for both ggez and wgpu renderers
#[allow(unused_imports)] // May be unused depending on renderer feature
pub use energy_window_manager::EnergyWindowManager;
#[allow(unused_imports)] // Unused when building without ggez
pub use event_loop_state::EventLoopState;
pub use input_handler::{InputAction, InputHandler, KeyCode};
pub use physics_coordinator::PhysicsCoordinator;
#[allow(unused_imports)] // May be used in future refactoring
pub use render_context::RenderContext;
