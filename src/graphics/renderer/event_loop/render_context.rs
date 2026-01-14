use crate::application::AppResult;

/// Abstract rendering context for backend-agnostic event loop
pub trait RenderContext {
    /// Begin a new frame
    fn begin_frame(&mut self) -> AppResult;

    /// End the current frame and present
    fn end_frame(&mut self) -> AppResult;

    /// Request application quit
    fn request_quit(&mut self);

    /// Check if quit was requested
    fn is_quit_requested(&self) -> bool;

    /// Check if application should continue running
    fn is_continuing(&self) -> bool;

    /// Notify that a frame tick occurred (for timing)
    fn tick(&mut self);
}
