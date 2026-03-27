//! WGPU rendering backend
//!
//! This module contains the wgpu-based renderer implementation.
//!
//! # Components
//! - [`WgpuContext`]: Core GPU resources (Device, Queue, Surface, Adapter)
//! - [`WgpuWindow`]: Window management and event loop
//! - [`WgpuPipelines`]: Collection of all rendering pipelines
//! - [`GridPipeline`]: Instanced grid rendering pipeline
//! - [`BallPipeline`]: Instanced ball rendering pipeline with storage buffers
//! - [`CrosshairPipeline`]: Crosshair and ball placement cursor pipeline
//! - [`FramePipeline`]: Screen-space frame border rendering pipeline
//! - [`TextPipeline`]: Text rendering pipeline using a texture atlas
//! - [`EnergyWindowManager`]: Manages the energy plot window lifecycle and data updates

mod ball_pipeline;
mod crosshair_pipeline;
mod frame_pipeline;
mod grid_pipeline;
mod line_pipeline;
mod render_graph;
mod sidebar_pipeline;
mod text_pipeline;
mod thermo_heatmap_pipeline;
mod thermo_value_pipeline;
mod wgpu_context;
mod wgpu_input;
mod wgpu_pipelines;
mod wgpu_window;

pub use wgpu_window::WgpuWindow;

use rust_thermo_core::graphics::renderer::viewport::Margins;

pub const MARGIN_LEFT: f32 = 110.;
pub const MARGIN_RIGHT: f32 = 50.;
pub const MARGIN_TOP: f32 = 50.;
pub const MARGIN_BOTTOM: f32 = 50.;

pub const MARGINS: Margins = Margins::new(MARGIN_LEFT, MARGIN_TOP, MARGIN_RIGHT, MARGIN_BOTTOM);
