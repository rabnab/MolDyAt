//! WGPU rendering backend
//!
//! This module contains the wgpu-based renderer implementation.
//! It provides an alternative to the ggez renderer with direct GPU control.
//!
//! # Components
//! - [`WgpuContext`]: Core GPU resources (Device, Queue, Surface, Adapter)
//! - [`WgpuWindow`]: Window management and event loop
//!
//! # Usage
//! Enable the `renderer-wgpu` feature and disable `renderer-ggez`:
//! ```bash
//! cargo run --no-default-features --features renderer-wgpu
//! ```

mod wgpu_context;
pub(crate) mod wgpu_window;

#[allow(unused_imports)] // Will be used by future renderer implementation
pub use wgpu_context::WgpuContext;
