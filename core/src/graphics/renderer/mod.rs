pub mod batch_renderer;
pub mod event_loop;
pub mod interpolation;
pub mod state_layers;
pub mod viewport;

// ── Public API re-exports ───────────────────────────────────────────
// These define the public interface of the renderer module. Renderers
// import from submodules directly, but these exports provide a stable
// API and document public types.

// Public API for batch rendering (used by both renderers)
#[allow(unused_imports)] // Public API, accessed via direct submodule imports
pub use batch_renderer::{
    BatchConfig, BatchRenderer, Batchable, LineSegment, collect_neighbor_lines,
};

// Public API for event handling - used by both ggez and wgpu event loops
#[allow(unused_imports)] // Public API, accessed via direct submodule imports
pub use event_loop::{InputAction, InputHandler, KeyCode};

// Public API for unified viewport transformation (used by both renderers)
#[allow(unused_imports)] // Public API, accessed via direct submodule imports
pub use viewport::{Margins, ViewportTransform};

// Public API for unified render state (used by both renderers)
#[allow(unused_imports)] // Public API, accessed via direct submodule imports
pub use event_loop::UnifiedRenderState;

// Public API for per-frame metrics (used internally)
#[allow(unused_imports)] // Public API, accessed via direct submodule imports
pub use event_loop::{FrameMetrics, MetricsCollector};

// Public API for renderer state layers
// RendererConfig is used by both renderers (via UnifiedRenderState)
// PlotContent, FrameState, CacheStats are ggez-specific
#[allow(unused_imports)] // Public API, accessed via direct submodule imports
pub use state_layers::{RendererConfig, ThermoDisplayMode};
