//! WGPU rendering backend
//!
//! This module contains the wgpu-based renderer implementation.
//! It provides an alternative to the ggez renderer with direct GPU control.
//!
//! # Components
//! - [`WgpuContext`]: Core GPU resources (Device, Queue, Surface, Adapter)
//! - [`WgpuWindow`]: Window management and event loop
//! - [`GridPipeline`]: Instanced grid rendering pipeline
//! - [`BallPipeline`]: Instanced ball rendering pipeline with storage buffers
//! - [`CrosshairPipeline`]: Crosshair and ball placement cursor pipeline
//!
//! # Usage
//! Enable the `renderer-wgpu` feature and disable `renderer-ggez`:
//! ```bash
//! cargo run --no-default-features --features renderer-wgpu
//! ```

mod ball_pipeline;
mod crosshair_pipeline;
mod frame_pipeline;
mod grid_pipeline;
mod line_pipeline;
mod text_pipeline;
mod wgpu_context;
pub(crate) mod wgpu_window;

// Re-exports for public API (allow unused for library-style exports)
#[allow(unused_imports)]
pub use ball_pipeline::{BallPipeline, BallUniforms, GpuBallData};
#[allow(unused_imports)]
pub use crosshair_pipeline::{CrosshairPipeline, CrosshairUniforms};
#[allow(unused_imports)]
pub use frame_pipeline::FramePipeline;
#[allow(unused_imports)]
pub use grid_pipeline::{GridPipeline, GridUniforms, build_view_proj_matrix};
#[allow(unused_imports)]
pub use line_pipeline::LinePipeline;
#[allow(unused_imports)]
pub use text_pipeline::TextPipeline;
#[allow(unused_imports)]
pub use wgpu_context::WgpuContext;
