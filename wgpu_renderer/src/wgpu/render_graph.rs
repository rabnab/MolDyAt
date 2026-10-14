// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
//!
//! Each pipeline belongs to one [`SceneLayers`] layer. A frame prepares and
//! draws only the pipelines of its layers, so with rendering off (F2) the
//! ball and thermo pipelines do no work at all.

use moldyat_core::application::AppResult;
use moldyat_core::application::renderer_snapshot::RendererSnapshot;
use moldyat_core::application::scene_layers::SceneLayers;
use moldyat_core::graphics::renderer::event_loop::FramePreviews;
use moldyat_core::graphics::renderer::margins::Margins;
use moldyat_core::graphics::renderer::state_layers::ThermoDisplayMode;
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
    pub show_ball_outline: bool,
    pub window_size: (u32, u32),
    /// Configured margins (sidebar and other UI in the margins).
    pub margin: Margins,
    /// Margins around the simulation drawing area: `margin` plus any
    /// letterbox bars (see `ViewportTransform::inset_margins`).
    pub inset: Margins,
    pub thermo_display: ThermoDisplayMode,
    /// All operator preview geometry for the current frame.
    pub operator_previews: FramePreviews,
    /// Layers drawn this frame. Pipelines outside them are skipped; a
    /// pipeline that mixes layers checks it for its own parts.
    pub layers: SceneLayers,
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

    /// Scene layer this pipeline draws; skipped in frames without it.
    fn layer(&self) -> SceneLayers {
        SceneLayers::BASE
    }

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
        log::trace!("RenderGraph: registered pipeline \"{}\"", pipeline.name());
        self.pipelines.push(pipeline);
        self.execution_order.push(handle);
        self.enabled.insert(handle);
        handle
    }

    #[cfg(test)]
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

    #[cfg(test)]
    /// Replace the execution order.
    ///
    /// Only handles that refer to registered pipelines are kept;
    /// out-of-range handles are silently dropped.
    pub fn set_execution_order(&mut self, order: Vec<PipelineHandle>) {
        let len = self.pipelines.len();
        self.execution_order = order.into_iter().filter(|h| h.0 < len).collect();
    }

    /// Whether `handle` runs in a frame drawing `layers`: enabled, and its
    /// layer is one of them.
    pub fn runs(&self, handle: PipelineHandle, layers: SceneLayers) -> bool {
        self.is_enabled(handle) && layers.contains(self.pipelines[handle.0].layer())
    }

    /// Prepare the pipelines that run in a frame drawing `ctx.layers`
    /// (upload uniforms / buffers).
    pub fn prepare_all(&mut self, ctx: &PrepareContext) -> AppResult {
        for idx in 0..self.execution_order.len() {
            let handle = self.execution_order[idx];
            if self.runs(handle, ctx.layers) {
                self.pipelines[handle.0].prepare(ctx)?;
            }
        }
        Ok(())
    }

    /// Render the pipelines that run in a frame drawing `layers` (the same
    /// as the prepared ones), switching viewports as needed.
    ///
    /// The `window_size` and `inset` margins give the inset viewport
    /// rectangle (the simulation drawing area, letterboxed).
    pub fn render_all<'rp>(
        &'rp self,
        render_pass: &mut wgpu::RenderPass<'rp>,
        window_size: (u32, u32),
        inset: Margins,
        layers: SceneLayers,
    ) {
        let (w, h) = (window_size.0 as f32, window_size.1 as f32);
        let mut current_mode: Option<ViewportMode> = None;

        for &handle in &self.execution_order {
            if !self.runs(handle, layers) {
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
                        let vp_w = (w - inset.horizontal()).max(1.0);
                        let vp_h = (h - inset.vertical()).max(1.0);
                        render_pass.set_viewport(inset.left, inset.top, vp_w, vp_h, 0.0, 1.0);
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

    #[cfg(test)]
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
        layer: SceneLayers,
    }

    impl MockPipeline {
        fn new(label: &'static str, mode: ViewportMode) -> Self {
            Self {
                label,
                mode,
                layer: SceneLayers::BASE,
            }
        }

        fn in_layer(label: &'static str, layer: SceneLayers) -> Self {
            Self {
                label,
                mode: ViewportMode::Inset,
                layer,
            }
        }
    }

    impl RenderPipeline for MockPipeline {
        fn name(&self) -> &'static str {
            self.label
        }
        fn viewport_mode(&self) -> ViewportMode {
            self.mode
        }
        fn layer(&self) -> SceneLayers {
            self.layer
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
    fn a_frame_runs_only_the_pipelines_of_its_layers() {
        let mut graph = RenderGraph::new();
        let operators = graph.add_pipeline(Box::new(MockPipeline::in_layer(
            "operators",
            SceneLayers::BASE,
        )));
        let balls = graph.add_pipeline(Box::new(MockPipeline::in_layer(
            "balls",
            SceneLayers::BALLS,
        )));
        let heatmap = graph.add_pipeline(Box::new(MockPipeline::in_layer(
            "heatmap",
            SceneLayers::THERMO,
        )));

        // Rendering on: everything.
        for handle in [operators, balls, heatmap] {
            assert!(graph.runs(handle, SceneLayers::ALL));
        }

        // Rendering off (F2): no ball or thermo work, operators still run.
        let light = SceneLayers::ALL.difference(SceneLayers::HEAVY);
        assert!(graph.runs(operators, light));
        assert!(!graph.runs(balls, light));
        assert!(!graph.runs(heatmap, light));

        // A disabled pipeline never runs.
        graph.set_enabled(operators, false);
        assert!(!graph.runs(operators, SceneLayers::ALL));
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
