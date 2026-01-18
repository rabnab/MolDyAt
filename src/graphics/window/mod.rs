mod window_core;
pub mod window_definition;

// Window trait is currently only used by ggez renderer
#[cfg(feature = "renderer-ggez")]
pub use window_core::Window;

pub struct Point2D(pub f32, pub f32);
pub enum WindowEvent {/* ... */}

pub const MARGIN_PX_X: f32 = 50.;
pub const MARGIN_PX_Y: f32 = 50.;
