// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! WGPU rendering backend, in two parts with a one-way dependency (#372):
//!
//! - [`shell`]: the OS side: the winit event loop, the main and plot
//!   windows, input dispatch into `UnifiedRenderState`, redraw requests
//! - [`render`]: the drawing side: the GPU context and surfaces, the
//!   pipelines and the render graph, one frame drawn without a window. It
//!   uses no winit (checked by `tests/render_guard.rs`).
//!
//! The shell depends on the renderer, never the other way round; what the
//! renderer needs of a window goes through
//! [`render::surface_window::RenderWindow`]. A later crate split (library
//! for the renderer, binary for the shell) is then a file move.

mod render;
mod shell;

#[cfg(test)]
mod tests;

pub use shell::wgpu_window::WgpuWindow;

use thermo_ui::graphics::renderer::margins::Margins;

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
