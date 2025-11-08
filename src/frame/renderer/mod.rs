mod dummy_renderer;
mod renderer;

pub use renderer::DrawingBackend;
pub use renderer::PlotContent;
pub use renderer::Renderer;

#[cfg(test)]
pub(crate) use renderer::create_dummy_renderer;
pub(crate) use renderer::is_number_continue;
pub(crate) use renderer::is_number_start;
