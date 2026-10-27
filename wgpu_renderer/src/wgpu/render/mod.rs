// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The renderer (#372): everything that draws with wgpu, and nothing that
//! talks to the OS. No file under `render/` uses winit; the shell
//! (`super::shell`) depends on this module, never the other way round, and
//! a test (`tests/render_guard.rs`) enforces it.
//!
//! - [`wgpu_context::WgpuContext`]: instance, adapter, device, queue and the
//!   main surface, created from raw window and display handles
//! - [`surface_window::RenderWindow`]: what the renderer needs of a window
//!   (raw handles, size, redraw, about-to-present); the shell implements it
//! - [`frame`]: one frame of the main window, drawn without a window
//! - [`plot_surface`]: the plot window's surface and its bookkeeping, keyed
//!   by an opaque window id the shell owns
//! - [`wgpu_pipelines::WgpuPipelines`]: all render pipelines, registered in
//!   a [`render_graph::RenderGraph`]
//! - `pipelines/`: one `RenderPipeline` per visual layer, all built on `gpu/`
//! - `gpu/`: shared building blocks: `GpuBatch` (the only place that creates
//!   wgpu pipelines and buffers), WGSL sources, textures, bitmap text

pub(crate) mod frame;
pub(crate) mod gpu;
pub(crate) mod pipelines;
pub(crate) mod plot_surface;
pub(crate) mod render_graph;
pub(crate) mod surface_window;
pub(crate) mod wgpu_context;
pub(crate) mod wgpu_pipelines;
