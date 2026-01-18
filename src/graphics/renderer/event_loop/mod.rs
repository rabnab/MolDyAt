mod event_loop_state;
mod input_handler;
mod physics_coordinator;
mod render_context;
#[cfg(test)]
mod tests;

// EventLoopState is used by ggez renderer; WGPU may use it in future refactoring
#[cfg(feature = "renderer-ggez")]
pub use event_loop_state::EventLoopState;
pub use input_handler::{InputAction, InputHandler, KeyCode};
pub use physics_coordinator::PhysicsCoordinator;
#[allow(unused_imports)] // Will be used in WGPU implementation
pub use render_context::RenderContext;
