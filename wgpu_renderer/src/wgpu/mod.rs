// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! WGPU rendering backend
//!
//! This module contains the wgpu-based renderer implementation.
//! After issue #140, the runtime wiring is split into focused pieces:
//! [`app_lifecycle::AppLifecycle`] bootstraps the window and renderer state,
//! [`event_router::EventRouter`] classifies and dispatches winit events, and
//! [`frame_renderer::FrameRenderer`] prepares pipelines and submits redraws.
//!
//! # Components
//! - [`app_lifecycle::AppLifecycle`]: Window, context, pipeline, and shared-state bootstrap
//! - [`event_router::EventRouter`]: Window and device event classification and dispatch
//! - [`frame_renderer::FrameRenderer`]: Snapshot preparation and frame submission
//! - [`WgpuContext`]: Core GPU resources (Device, Queue, Surface, Adapter)
//! - [`WgpuWindow`]: Thin event-loop shell that wires lifecycle, routing, and rendering
//! - [`WgpuPipelines`]: All render pipelines, registered in a `RenderGraph`
//! - `pipelines/`: one `RenderPipeline` per visual layer (balls, walls,
//!   sidebar, labels, ...), all built the same way on top of `gpu/`
//! - `gpu/`: shared building blocks — `GpuBatch` (the only place that creates
//!   wgpu pipelines and buffers), WGSL sources with the common header,
//!   textures, bitmap text
//! - `EnergyWindowManager`: Managed through the shared core renderer state layer

mod app_lifecycle;
mod event_router;
mod frame_renderer;
mod gpu;
mod pipelines;
mod render_graph;
mod wgpu_context;
mod wgpu_input;
mod wgpu_pipelines;
mod wgpu_window;

#[cfg(test)]
mod tests;

pub use wgpu_window::WgpuWindow;

use rust_thermo_core::graphics::renderer::margins::Margins;

pub const DEFAULT_WINDOW_WIDTH: f64 = 1280.0;
pub const DEFAULT_WINDOW_HEIGHT: f64 = 720.0;

pub const MARGIN_LEFT: f32 = 110.;
pub const MARGIN_RIGHT: f32 = 50.;
pub const MARGIN_TOP: f32 = 50.;
pub const MARGIN_BOTTOM: f32 = 50.;

pub const MARGINS: Margins = Margins::new(MARGIN_LEFT, MARGIN_TOP, MARGIN_RIGHT, MARGIN_BOTTOM);

pub fn ui_scale_factor(scale_factor: f64) -> f32 {
    scale_factor.max(1.0) as f32
}

pub fn margins_for_scale_factor(scale_factor: f64) -> Margins {
    let ui_scale = ui_scale_factor(scale_factor);
    Margins::new(
        MARGIN_LEFT * ui_scale,
        MARGIN_TOP * ui_scale,
        MARGIN_RIGHT * ui_scale,
        MARGIN_BOTTOM * ui_scale,
    )
}
