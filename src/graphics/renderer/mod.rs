mod dummy_renderer;
pub mod event_loop;
mod renderer_core;
pub mod viewport;

pub use renderer_core::DrawingBackend;
pub use renderer_core::PlotContent;
pub use renderer_core::Renderer;
// Public API for event handling - will be used when integrating with event loops
#[allow(unused_imports)]
pub use event_loop::{InputAction, InputHandler, KeyCode};

// These are currently only used by ggez renderer
#[cfg(feature = "renderer-ggez")]
pub use renderer_core::BallType;

#[cfg(feature = "renderer-ggez")]
pub(crate) use renderer_core::is_number_continue;
#[cfg(feature = "renderer-ggez")]
pub(crate) use renderer_core::is_number_start;
