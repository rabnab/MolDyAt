pub mod event_loop;
pub mod viewport;

// Public API for event handling - used by both ggez and wgpu event loops
#[allow(unused_imports)]
pub use event_loop::{InputAction, InputHandler, KeyCode};

// Public API for unified viewport transformation
#[allow(unused_imports)]
pub use viewport::ViewportTransform;
