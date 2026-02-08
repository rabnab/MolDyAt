//! Render graph for structured wgpu pipeline management.
//!
//! Provides a [`RenderPipeline`] trait and a [`RenderGraph`] that manages
//! pipeline registration, execution order, and enable/disable toggling
//! at runtime.
//!
//! # Architecture
//!
//! Each rendering stage (grid, balls, lines, …) implements
//! [`RenderPipeline`].  The [`RenderGraph`] owns all pipelines and
//! iterates over them in a configurable order, switching viewports
//! as indicated by each pipeline's [`ViewportMode`].
//!
//! ```text
//! RenderGraph
//!   ├─ prepare_all(ctx)   →  calls prepare() on each enabled pipeline
//!   └─ render_all(pass, …) →  sets viewport → calls render() on each enabled pipeline
//! ```

use crate::application::AppResult;
use crate::application::renderer_snapshot::RendererSnapshot;
use std::any::Any;
use std::collections::HashSet;

// ── Viewport mode ──────────────────────────────────────────────────

/// Viewport region a pipeline renders into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportMode {
    /// Full window (used by frame border, text overlay).
    FullWindow,
    /// Inset simulation pane with margins applied.
    Inset,
}

// ── Prepare context ────────────────────────────────────────────────

/// All data a pipeline needs during its `prepare()` phase.
///
/// Bundles snapshot, GPU resources, and viewport parameters into a
/// single struct so individual pipelines don't need 7+ arguments.
pub struct PrepareContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub snapshot: &'a RendererSnapshot,
    pub view_proj: [[f32; 4]; 4],
    pub cursor_world_pos: (f32, f32),
    pub current_ball_radius: f32,
    pub ctrl_pressed: bool,
    pub window_size: (u32, u32),
    pub margin: (f32, f32),
}

// ── Pipeline trait ─────────────────────────────────────────────────

/// Trait for individual render pipelines.
///
/// Each pipeline:
/// 1. Declares which viewport it renders into ([`viewport_mode`](Self::viewport_mode)).
/// 2. Uploads data / updates uniforms in [`prepare`](Self::prepare).
/// 3. Records draw commands in [`render`](Self::render).
///
/// The `Any` super-trait enables safe downcasting via
/// [`RenderGraph::get_pipeline_mut`].
pub trait RenderPipeline: Any {
    /// Human-readable name for debugging and profiling.
    fn name(&self) -> &'static str;

    /// Which viewport region this pipeline renders into.
    fn viewport_mode(&self) -> ViewportMode;

    /// Update uniforms / upload buffers from snapshot data.
    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult;

    /// Record render commands into an active render pass.
    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>);
}

// ── Pipeline handle ────────────────────────────────────────────────

/// Opaque handle to a pipeline registered in a [`RenderGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PipelineHandle(usize);

// ── Render graph ───────────────────────────────────────────────────

/// Orchestrates pipeline preparation and rendering.
///
/// Pipelines are stored in registration order.  An explicit
/// `execution_order` vector controls rendering sequence and can be
/// changed at runtime.  Individual pipelines can be enabled/disabled
/// without affecting order.
pub struct RenderGraph {
    pipelines: Vec<Box<dyn RenderPipeline>>,
    execution_order: Vec<PipelineHandle>,
    enabled: HashSet<PipelineHandle>,
}

impl RenderGraph {
    /// Create an empty render graph.
    pub fn new() -> Self {
        Self {
            pipelines: Vec::new(),
            execution_order: Vec::new(),
            enabled: HashSet::new(),
        }
    }

    /// Register a pipeline and return its handle.
    ///
    /// The pipeline is appended to the execution order and enabled by
    /// default.
    pub fn add_pipeline(&mut self, pipeline: Box<dyn RenderPipeline>) -> PipelineHandle {
        let handle = PipelineHandle(self.pipelines.len());
        println!(
            "  ✓ RenderGraph: registered pipeline \"{}\"",
            pipeline.name()
        );
        self.pipelines.push(pipeline);
        self.execution_order.push(handle);
        self.enabled.insert(handle);
        handle
    }

    /// Enable or disable a pipeline.
    ///
    /// Returns `false` if the handle does not belong to this graph.
    pub fn set_enabled(&mut self, handle: PipelineHandle, enabled: bool) -> bool {
        if handle.0 >= self.pipelines.len() {
            return false;
        }
        if enabled {
            self.enabled.insert(handle);
        } else {
            self.enabled.remove(&handle);
        }
        true
    }

    /// Check whether a pipeline is enabled.
    ///
    /// Returns `false` for handles that do not belong to this graph.
    pub fn is_enabled(&self, handle: PipelineHandle) -> bool {
        handle.0 < self.pipelines.len() && self.enabled.contains(&handle)
    }

    /// Replace the execution order.
    ///
    /// Only handles that refer to registered pipelines are kept;
    /// out-of-range handles are silently dropped.
    pub fn set_execution_order(&mut self, order: Vec<PipelineHandle>) {
        let len = self.pipelines.len();
        self.execution_order = order.into_iter().filter(|h| h.0 < len).collect();
    }

    /// Prepare all enabled pipelines (upload uniforms / buffers).
    pub fn prepare_all(&mut self, ctx: &PrepareContext) -> AppResult {
        for &handle in &self.execution_order {
            if self.enabled.contains(&handle) {
                self.pipelines[handle.0].prepare(ctx)?;
            }
        }
        Ok(())
    }

    /// Render all enabled pipelines, switching viewports as needed.
    ///
    /// The `window_size` and `margin` are used to compute the inset
    /// viewport rectangle.
    pub fn render_all<'rp>(
        &'rp self,
        render_pass: &mut wgpu::RenderPass<'rp>,
        window_size: (u32, u32),
        margin: (f32, f32),
    ) {
        let (w, h) = (window_size.0 as f32, window_size.1 as f32);
        let mut current_mode: Option<ViewportMode> = None;

        for &handle in &self.execution_order {
            if !self.enabled.contains(&handle) {
                continue;
            }
            let pipeline = &self.pipelines[handle.0];
            let mode = pipeline.viewport_mode();

            // Switch viewport only when the mode changes.
            if current_mode != Some(mode) {
                match mode {
                    ViewportMode::FullWindow => {
                        render_pass.set_viewport(0.0, 0.0, w, h, 0.0, 1.0);
                    }
                    ViewportMode::Inset => {
                        let vp_w = (w - 2.0 * margin.0).max(1.0);
                        let vp_h = (h - 2.0 * margin.1).max(1.0);
                        render_pass.set_viewport(margin.0, margin.1, vp_w, vp_h, 0.0, 1.0);
                    }
                }
                current_mode = Some(mode);
            }

            pipeline.render(render_pass);
        }
    }

    /// Get a mutable reference to a pipeline by handle, downcast to a
    /// concrete type.
    ///
    /// This is useful for pipeline-specific methods that are not part of
    /// the [`RenderPipeline`] trait (e.g., `BallPipeline::toggle_cutoff_rings`).
    ///
    /// Returns `None` if the handle is invalid or the downcast fails.
    pub fn get_pipeline_mut<T: RenderPipeline + 'static>(
        &mut self,
        handle: PipelineHandle,
    ) -> Option<&mut T> {
        self.pipelines.get_mut(handle.0).and_then(|p| {
            let any: &mut dyn Any = p.as_mut();
            any.downcast_mut::<T>()
        })
    }

    /// Number of registered pipelines.
    pub fn pipeline_count(&self) -> usize {
        self.pipelines.len()
    }

    /// Number of currently enabled pipelines.
    pub fn enabled_count(&self) -> usize {
        self.execution_order
            .iter()
            .filter(|h| self.enabled.contains(h))
            .count()
    }
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal mock pipeline that records calls without needing a GPU.
    struct MockPipeline {
        label: &'static str,
        mode: ViewportMode,
    }

    impl MockPipeline {
        fn new(label: &'static str, mode: ViewportMode) -> Self {
            Self { label, mode }
        }
    }

    impl RenderPipeline for MockPipeline {
        fn name(&self) -> &'static str {
            self.label
        }
        fn viewport_mode(&self) -> ViewportMode {
            self.mode
        }
        fn prepare(&mut self, _ctx: &PrepareContext) -> AppResult {
            Ok(())
        }
        fn render<'rp>(&'rp self, _render_pass: &mut wgpu::RenderPass<'rp>) {}
    }

    #[test]
    fn add_pipeline_returns_sequential_handles() {
        let mut graph = RenderGraph::new();
        let h0 = graph.add_pipeline(Box::new(MockPipeline::new("a", ViewportMode::FullWindow)));
        let h1 = graph.add_pipeline(Box::new(MockPipeline::new("b", ViewportMode::Inset)));

        assert_eq!(h0, PipelineHandle(0));
        assert_eq!(h1, PipelineHandle(1));
        assert_ne!(h0, h1);
        assert_eq!(graph.pipeline_count(), 2);
        assert_eq!(graph.enabled_count(), 2);
        assert!(graph.is_enabled(h0));
        assert!(graph.is_enabled(h1));
    }

    #[test]
    fn disable_pipeline_excludes_from_enabled_count() {
        let mut graph = RenderGraph::new();
        let h0 = graph.add_pipeline(Box::new(MockPipeline::new("a", ViewportMode::FullWindow)));
        let _h1 = graph.add_pipeline(Box::new(MockPipeline::new("b", ViewportMode::Inset)));

        assert!(graph.set_enabled(h0, false));
        assert!(!graph.is_enabled(h0));
        assert_eq!(graph.enabled_count(), 1);

        // Re-enable
        assert!(graph.set_enabled(h0, true));
        assert!(graph.is_enabled(h0));
        assert_eq!(graph.enabled_count(), 2);
    }

    #[test]
    fn invalid_handle_is_rejected() {
        let mut graph = RenderGraph::new();
        let bogus = PipelineHandle(42);

        assert!(!graph.set_enabled(bogus, true));
        assert!(!graph.is_enabled(bogus));
    }

    #[test]
    fn set_execution_order_filters_invalid_handles() {
        let mut graph = RenderGraph::new();
        let h0 = graph.add_pipeline(Box::new(MockPipeline::new("a", ViewportMode::FullWindow)));
        let bogus = PipelineHandle(99);

        graph.set_execution_order(vec![bogus, h0, bogus]);
        // Only h0 survives filtering
        assert_eq!(graph.enabled_count(), 1);
    }

    #[test]
    fn downcast_returns_none_for_wrong_type() {
        let mut graph = RenderGraph::new();
        let h0 = graph.add_pipeline(Box::new(MockPipeline::new("a", ViewportMode::FullWindow)));

        // Downcast to a different concrete type should return None
        struct OtherPipeline;
        impl RenderPipeline for OtherPipeline {
            fn name(&self) -> &'static str {
                "other"
            }
            fn viewport_mode(&self) -> ViewportMode {
                ViewportMode::FullWindow
            }
            fn prepare(&mut self, _ctx: &PrepareContext) -> AppResult {
                Ok(())
            }
            fn render<'rp>(&'rp self, _render_pass: &mut wgpu::RenderPass<'rp>) {}
        }

        assert!(graph.get_pipeline_mut::<OtherPipeline>(h0).is_none());
        assert!(graph.get_pipeline_mut::<MockPipeline>(h0).is_some());
    }

    #[test]
    fn default_is_empty() {
        let graph = RenderGraph::default();
        assert_eq!(graph.pipeline_count(), 0);
        assert_eq!(graph.enabled_count(), 0);
    }

    #[test]
    fn viewport_mode_variants() {
        assert_ne!(ViewportMode::FullWindow, ViewportMode::Inset);
        assert_eq!(ViewportMode::FullWindow, ViewportMode::FullWindow);
        assert_eq!(ViewportMode::Inset, ViewportMode::Inset);
    }

    #[test]
    fn pipeline_handle_equality() {
        let h0 = PipelineHandle(0);
        let h1 = PipelineHandle(1);
        let h0b = PipelineHandle(0);
        assert_eq!(h0, h0b);
        assert_ne!(h0, h1);
    }
}
