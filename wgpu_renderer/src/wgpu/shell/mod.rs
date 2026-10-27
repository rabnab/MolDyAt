// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The app shell (#372): the winit event loop, the windows and input. It
//! creates the windows, hands their events to `UnifiedRenderState`
//! (`thermo_ui`) and asks the renderer (`super::render`) to draw; the
//! renderer never sees a winit type.
//!
//! - [`wgpu_window::WgpuWindow`]: the `ApplicationHandler` that wires it all
//! - [`app_lifecycle::AppLifecycle`]: main window, GPU context, pipelines
//!   and render state at start
//! - [`event_router::EventRouter`]: window and device events to their
//!   handlers
//! - [`frame_renderer::FrameRenderer`]: the window side of a frame (redraw
//!   requests, sizes, surface errors)
//! - [`plot_window`]: the plot window's OS window, created and closed as
//!   `thermo_ui` asks (#371)
//! - [`winit_window`]: winit's `Window` as the renderer's `RenderWindow`

pub(crate) mod app_lifecycle;
pub(crate) mod event_router;
pub(crate) mod frame_renderer;
pub(crate) mod plot_window;
pub(crate) mod wgpu_input;
pub(crate) mod wgpu_window;
pub(crate) mod winit_window;
