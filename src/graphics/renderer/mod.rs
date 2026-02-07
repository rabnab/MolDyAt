pub mod batch_renderer;
pub mod event_loop;
pub mod viewport;

// Public API for batch rendering
#[allow(unused_imports)]
pub use batch_renderer::{
    BatchConfig, BatchRenderer, Batchable, LineSegment, collect_neighbor_lines,
};

// Public API for event handling - used by both ggez and wgpu event loops
#[allow(unused_imports)]
pub use event_loop::{InputAction, InputHandler, KeyCode};

// Public API for unified viewport transformation
#[allow(unused_imports)]
pub use viewport::ViewportTransform;

// Public API for unified render state
#[allow(unused_imports)]
pub use event_loop::UnifiedRenderState;

// Public API for per-frame metrics
#[allow(unused_imports)]
pub use event_loop::{FrameMetrics, MetricsCollector};
