pub mod coordinate_transform;
pub mod event_loop;
pub mod viewport;

// Public API for event handling - used by both ggez and wgpu event loops
#[allow(unused_imports)]
pub use event_loop::{InputAction, InputHandler, KeyCode};

// Public API for coordinate transformations - shared by ggez and wgpu
// Currently only used by wgpu, but available for ggez refactoring
#[allow(unused_imports)]
pub use coordinate_transform::{CoordinateTransform, compute_world_rect_from_window_size};
