mod input_handler;
mod physics_coordinator;
#[cfg(test)]
mod tests;

pub use input_handler::{InputAction, InputHandler, KeyCode};
// PhysicsCoordinator will be used in a future PR (issue #74)
#[allow(unused_imports)]
pub use physics_coordinator::PhysicsCoordinator;
