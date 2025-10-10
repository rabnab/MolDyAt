mod window;
pub mod window_definition;

pub use window::Window;

pub struct Point2D(pub f32, pub f32);
pub enum WindowEvent {/* ... */}

pub const MARGIN_PX_X: f32 = 50.;
pub const MARGIN_PX_Y: f32 = 50.;
