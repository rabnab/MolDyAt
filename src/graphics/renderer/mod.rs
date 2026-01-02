mod dummy_renderer;
mod renderer_core;
pub mod viewport;

pub use renderer_core::BallType;
pub use renderer_core::DrawingBackend;
pub use renderer_core::PlotContent;
pub use renderer_core::Renderer;

pub(crate) use renderer_core::is_number_continue;
pub(crate) use renderer_core::is_number_start;
