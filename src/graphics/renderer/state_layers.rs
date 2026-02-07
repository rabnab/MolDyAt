//! Renderer state layers — shared by both ggez and wgpu backends.
//!
//! Separates renderer state into three explicit categories:
//!
//! - [`RendererConfig`] — stable across many frames (user toggles,
//!   debug switches).
//! - [`FrameState`] — transient per-frame data, reset between renders.
//! - [`RenderCacheStats`] — GPU-agnostic cache metrics (each backend
//!   wraps its own cache type).
//!
//! This module intentionally contains no ggez or wgpu types so that
//! unit tests can exercise state logic without a GPU context.

// TODO: Populate with RendererConfig, FrameState, RenderCacheStats
//       as fields are migrated from GgezRenderer and wgpu pipelines.
