// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared GPU building blocks for the render pipelines.
//!
//! Nothing here implements `RenderPipeline`; the pipelines in `pipelines/`
//! are thin users of these pieces:
//! - [`batch`]: [`GpuBatch`](batch::GpuBatch), the only place that creates
//!   render pipelines, bind groups and vertex/instance buffers
//! - [`shaders`]: every WGSL source, with `common.wgsl` prepended
//! - [`textures`]: embedded icons and texture helpers
//! - [`text`], [`label_background`]: bitmap text and label capsules
//! - [`geometry`]: CPU-side matrix and coordinate helpers

pub(crate) mod batch;
pub(crate) mod geometry;
pub(crate) mod label_background;
pub(crate) mod shaders;
pub(crate) mod text;
pub(crate) mod textures;

#[cfg(test)]
mod tests;
